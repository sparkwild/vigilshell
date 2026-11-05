import { EventEmitter } from 'node:events';
import type {
  NotificationAuthorization,
  NotificationOutcome,
  Preferences,
  SystemNotificationResult,
  SystemNotificationStatus,
} from '../../contracts/generated/runtime';

export interface SystemNotificationIntent {
  eventId: string;
  stillRelevant?: () => boolean;
  category: 'attentionRequired' | 'executionIssue';
  title: string;
  body: string;
}

export interface NotificationPorts {
  supported(): boolean;
  authorization(request: boolean): Promise<NotificationAuthorization>;
  preferences(): Pick<
    Preferences,
    | 'systemNotificationMode'
    | 'attentionRequiredNotificationEnabled'
    | 'executionIssueNotificationEnabled'
  > | null;
  focused(): boolean;
  deliver(message: { id: string; title: string; body: string }): Promise<NotificationOutcome>;
  openSettings(): Promise<void>;
  dispose(): void;
}

/** 最近事件去重仅属于当前 Main 运行期；未知结果不会被重放。 */
export class SystemNotificationService extends EventEmitter<{
  changed: [SystemNotificationStatus];
}> {
  private status: SystemNotificationStatus = { authorization: 'unknown', version: 0 };
  private query: Promise<SystemNotificationStatus> | null = null;
  private permission: Promise<SystemNotificationStatus> | null = null;
  private readSequence = 0;
  private stopped = false;
  private permissionAttempted = false;
  private seen = new Set<string>();

  constructor(private readonly ports: NotificationPorts) {
    super();
  }

  getStatus(): Promise<SystemNotificationStatus> {
    if (this.permission) return this.permission;
    if (this.query) return this.query;
    this.query = this.readAuthorization(false).finally(() => {
      this.query = null;
    });
    return this.query;
  }

  requestPermission(): Promise<SystemNotificationStatus> {
    if (this.permission) return this.permission;
    this.permission = (async () => {
      const state = await (this.query ?? this.readAuthorization(false));
      if (state.authorization !== 'notDetermined' || this.stopped) return state;
      return this.readAuthorization(true);
    })().finally(() => {
      this.permission = null;
    });
    return this.permission;
  }

  private async readAuthorization(request: boolean): Promise<SystemNotificationStatus> {
    if (this.stopped) return this.status;
    const sequence = ++this.readSequence;
    let authorization: NotificationAuthorization = 'unsupported';
    if (this.ports.supported()) {
      try {
        authorization = await this.ports.authorization(request);
      } catch {
        authorization = 'unknown';
      }
    }
    if (!this.stopped && sequence === this.readSequence) {
      this.status = { authorization, version: this.status.version + 1 };
      this.emit('changed', this.status);
    }
    return this.status;
  }

  async publish(intent: SystemNotificationIntent): Promise<SystemNotificationResult> {
    if (this.stopped) return this.result('stopped');
    if (
      !intent.eventId ||
      !intent.title ||
      intent.eventId.length > 256 ||
      intent.title.length > 256 ||
      intent.body.length > 1024
    )
      return this.result('failed');
    if (this.seen.has(intent.eventId)) return this.result('duplicate');
    this.seen.add(intent.eventId);
    if (this.seen.size > 256) this.seen.delete(this.seen.values().next().value!);
    if (!this.allowed(intent)) return this.result('suppressed');
    await this.getStatus();
    if (this.stopped) return this.result('stopped');
    if (
      this.status.authorization === 'notDetermined' &&
      !this.permissionAttempted &&
      this.allowed(intent)
    ) {
      this.permissionAttempted = true;
      await this.requestPermission();
    } else if (this.permission) {
      await this.permission;
    }
    if (this.stopped) return this.result('stopped');
    if (!this.allowed(intent) || !this.authorized()) return this.result('suppressed');
    return this.deliver({ id: intent.eventId, title: intent.title, body: intent.body });
  }

  private allowed(intent: SystemNotificationIntent) {
    const p = this.ports.preferences();
    if (
      !p ||
      p.systemNotificationMode === 'off' ||
      (p.systemNotificationMode === 'unfocused' && this.ports.focused())
    )
      return false;
    const enabled =
      intent.category === 'attentionRequired'
        ? p.attentionRequiredNotificationEnabled
        : p.executionIssueNotificationEnabled;
    return enabled && (!intent.stillRelevant || intent.stillRelevant());
  }

  async openSystemSettings(): Promise<SystemNotificationResult> {
    if (this.stopped) return this.result('stopped');
    if (!this.ports.supported()) return this.result('suppressed');
    try {
      await this.ports.openSettings();
      return this.result('opened');
    } catch {
      return this.result('failed');
    }
  }

  stop(): void {
    this.stopped = true;
    this.readSequence++;
    this.ports.dispose();
    this.removeAllListeners();
  }

  private authorized() {
    return ['authorized', 'provisional'].includes(this.status.authorization);
  }
  private result(outcome: NotificationOutcome): SystemNotificationResult {
    return { outcome, status: this.status };
  }
  private async deliver(message: { id: string; title: string; body: string }) {
    try {
      return this.result(await this.ports.deliver(message));
    } catch {
      return this.result('failed');
    }
  }
}
