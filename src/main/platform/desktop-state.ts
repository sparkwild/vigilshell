import { EventEmitter } from 'node:events';
import type { DesktopSnapshot, Preferences, UpdatePreferencesRequest } from '@contracts';
import { resolveLocale } from '../../shared/i18n/locale';
interface AppearancePort {
  setThemeMode(mode: Preferences['themeMode']): void;
  currentTheme(): 'light' | 'dark';
  preferredLanguages(): string[];
  reducedTransparency(): boolean;
}
import { BackendClient, BackendError } from '../backend/client';
import { isPreferencesUpdate } from '../backend/validation';
import type { RpcValue, WriteOutcome } from '@contracts';
type SaveOperation = {
  id: string;
  runtimeId: string;
  confirmation?: Promise<DesktopSnapshot>;
};
/** Main 拥有保存生命周期；页面卸载和请求超时都不会释放未解决的写入。 */
export class DesktopState extends EventEmitter<{ changed: [DesktopSnapshot] }> {
  private snapshot: DesktopSnapshot;
  private startPromise: Promise<DesktopSnapshot> | null = null;
  private saving: SaveOperation | null = null;
  private quitting = false;
  constructor(
    private readonly runtime: BackendClient,
    private readonly appearance: AppearancePort,
  ) {
    super();
    this.snapshot = {
      version: 0,
      save: { phase: 'idle', error: null },
      runtime: { status: 'starting', instanceId: null },
      preferences: null,
      appearance: {
        theme: appearance.currentTheme(),
        locale: resolveLocale('system', appearance.preferredLanguages()),
        reducedTransparency: appearance.reducedTransparency(),
      },
      error: null,
    };
    runtime.on('unavailable', () => {
      if (!this.quitting)
        this.publish({
          runtime: { status: 'unavailable', instanceId: null },
          error: 'runtime_unavailable',
          ...(this.saving ? { save: { phase: 'unknown', error: 'runtime_unavailable' } } : {}),
        });
    });
    runtime.on('changed', (p) => this.acceptPreferences(p));
    runtime.on('late', (event) => {
      const op = this.saving;
      if (!op || op.id !== event.id || op.runtimeId !== runtime.instanceId) return;
      if (event.error) this.complete(op, undefined, event.error);
      else if (event.result) this.complete(op, event.result);
    });
  }
  recover() {
    return this.start();
  }
  refreshSystem() {
    this.refreshAppearance();
  }
  update(request: UpdatePreferencesRequest, nativeDirectory = false) {
    if (
      !nativeDirectory &&
      (request?.patch?.downloadDirectory !== undefined ||
        request?.base?.downloadDirectory !== undefined ||
        request?.patch?.fileEditorApplication !== undefined ||
        request?.base?.fileEditorApplication !== undefined)
    )
      return Promise.resolve({
        ...this.getSnapshot(),
        error: 'invalid_request',
      });
    return this.updatePreferences(request);
  }
  getSnapshot(): DesktopSnapshot {
    return structuredClone(this.snapshot);
  }
  start(): Promise<DesktopSnapshot> {
    if (this.startPromise) return this.startPromise;
    if (this.quitting || this.snapshot.runtime.status === 'ready')
      return Promise.resolve(this.getSnapshot());
    this.publish({
      runtime: { status: 'starting', instanceId: null },
      error: null,
    });
    this.startPromise = this.startRuntime().finally(() => {
      this.startPromise = null;
    });
    return this.startPromise;
  }
  async restartBackend(): Promise<DesktopSnapshot> {
    if (this.startPromise) return this.startPromise;
    this.publish({ runtime: { status: 'starting', instanceId: null } });
    this.startPromise = (async () => {
      await this.runtime.stop();
      return this.startRuntime();
    })().finally(() => {
      this.startPromise = null;
    });
    return this.startPromise;
  }
  async updatePreferences(request: UpdatePreferencesRequest): Promise<DesktopSnapshot> {
    if (!isPreferencesUpdate(request)) return { ...this.getSnapshot(), error: 'invalid_request' };
    if (this.saving) return { ...this.getSnapshot(), error: 'busy' };
    if (this.snapshot.runtime.status !== 'ready' || this.quitting)
      return { ...this.getSnapshot(), error: 'runtime_unavailable' };
    const call = this.runtime.call({
      method: 'preferences.update',
      params: request,
    });
    const op: SaveOperation = {
      id: call.id,
      runtimeId: this.runtime.instanceId!,
    };
    this.saving = op;
    this.publish({ save: { phase: 'saving', error: null }, error: null });
    try {
      this.complete(op, await call.result);
    } catch (error) {
      if (this.saving !== op) return this.getSnapshot();
      if (error instanceof BackendError && error.code === 'timeout') return this.confirm(op);
      this.complete(
        op,
        undefined,
        error instanceof BackendError ? error : new BackendError('runtime_unavailable'),
      );
    }
    return this.getSnapshot();
  }
  reconcilePreferences(): Promise<DesktopSnapshot> {
    return this.saving ? this.confirm(this.saving) : Promise.resolve(this.getSnapshot());
  }
  private confirm(op: SaveOperation): Promise<DesktopSnapshot> {
    if (op.confirmation) return op.confirmation;
    if (this.saving !== op) return Promise.resolve(this.getSnapshot());
    this.publish({ save: { phase: 'confirming', error: null } });
    op.confirmation = (async () => {
      try {
        const r = await this.runtime.request({
          method: 'preferences.reconcile',
          params: { requestId: op.id, runtimeId: op.runtimeId },
        });
        if (this.saving !== op) return this.getSnapshot();
        if (r.kind !== 'reconciled') throw new BackendError('protocol_mismatch');
        this.acceptPreferences(r.preferences);
        if (r.outcome) this.completeOutcome(op, r.outcome);
        else {
          this.runtime.release(op.id);
          this.saving = null;
          this.publish({
            save: { phase: 'idle', error: 'result_unknown' },
            error: 'result_unknown',
          });
        }
      } catch (error) {
        if (this.saving === op)
          this.publish({
            save: {
              phase: 'unknown',
              error: error instanceof BackendError ? error.code : 'runtime_unavailable',
            },
            error: 'result_unknown',
          });
      }
      return this.getSnapshot();
    })().finally(() => {
      delete op.confirmation;
    });
    return op.confirmation;
  }
  private completeOutcome(op: SaveOperation, outcome: WriteOutcome) {
    if (outcome.error)
      this.complete(
        op,
        undefined,
        new BackendError(
          outcome.error.data.code,
          outcome.error.message,
          outcome.error.data.preferences ?? undefined,
        ),
      );
    else if (outcome.preferences && outcome.status !== 'rejected')
      this.complete(op, {
        kind: 'updated',
        outcome: outcome.status,
        preferences: outcome.preferences,
      });
  }
  private complete(op: SaveOperation, result?: RpcValue, error?: BackendError) {
    if (this.saving !== op) return;
    if (error && ['runtime_unavailable', 'protocol_mismatch'].includes(error.code)) {
      this.publish({
        save: { phase: 'unknown', error: error.code },
        error: error.code,
      });
      return;
    }
    this.runtime.release(op.id);
    this.saving = null;
    if (result?.kind === 'updated') this.acceptPreferences(result.preferences);
    if (error?.preferences) this.acceptPreferences(error.preferences);
    this.publish({
      save: { phase: 'idle', error: error?.code ?? null },
      error: error?.code ?? null,
    });
  }
  private acceptPreferences(p: Preferences) {
    if (this.snapshot.preferences && p.revision < this.snapshot.preferences.revision) return;
    this.publish({ preferences: p });
  }
  refreshAppearance(): void {
    const next = this.resolveAppearance();
    if (JSON.stringify(next) !== JSON.stringify(this.snapshot.appearance))
      this.publish({ appearance: next });
  }
  async stop() {
    this.quitting = true;
    await this.runtime.stop();
  }
  private async startRuntime(): Promise<DesktopSnapshot> {
    const unresolved = Boolean(this.saving);
    try {
      await this.runtime.start();
      const r = await this.runtime.request({
        method: 'preferences.get',
        params: {},
      });
      if (r.kind !== 'preferences') throw new BackendError('protocol_mismatch');
      if (this.quitting) {
        await this.runtime.stop();
        return this.getSnapshot();
      }
      this.saving = null;
      return this.publish({
        runtime: { status: 'ready', instanceId: this.runtime.instanceId },
        preferences: r.preferences,
        save: { phase: 'idle', error: unresolved ? 'result_unknown' : null },
        error: unresolved ? 'result_unknown' : null,
      });
    } catch (error) {
      return this.publish({
        runtime: { status: 'unavailable', instanceId: null },
        error: error instanceof BackendError ? error.code : 'runtime_unavailable',
      });
    }
  }
  private resolveAppearance(): DesktopSnapshot['appearance'] {
    this.appearance.setThemeMode(this.snapshot.preferences?.themeMode ?? 'system');
    return {
      theme: this.appearance.currentTheme(),
      locale: resolveLocale(
        this.snapshot.preferences?.localeMode ?? 'system',
        this.appearance.preferredLanguages(),
      ),
      reducedTransparency: this.appearance.reducedTransparency(),
    };
  }
  private publish(patch: Partial<DesktopSnapshot>): DesktopSnapshot {
    this.snapshot = {
      ...this.snapshot,
      ...patch,
      version: this.snapshot.version + 1,
    };
    this.snapshot.appearance = this.resolveAppearance();
    const snapshot = this.getSnapshot();
    this.emit('changed', snapshot);
    return snapshot;
  }
}
