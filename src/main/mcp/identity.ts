import { randomUUID } from 'node:crypto';

export interface IdentityPurpose {
  challengeId: string;
  action: string;
  sessionId?: string;
}
export type NativeIdentityOutcome =
  | 'approved'
  | 'userCancelled'
  | 'systemCancelled'
  | 'applicationCancelled'
  | 'authenticationFailed'
  | 'unavailable'
  | 'invalidContext'
  | 'nativeFailure';
export interface NativeIdentityResult {
  outcome: NativeIdentityOutcome;
  nativeCode: number;
}
export interface NativeIdentityBridge {
  verify(reason: string, callback: (result: NativeIdentityResult) => void): void;
  /** Returns a result already completed natively, before cancelling a still-pending request. */
  cancel(): NativeIdentityResult | undefined;
}
const codes: Record<Exclude<NativeIdentityOutcome, 'approved'>, string> = {
  userCancelled: 'authorization_cancelled',
  systemCancelled: 'authorization_interrupted',
  applicationCancelled: 'authorization_interrupted',
  authenticationFailed: 'authorization_failed',
  unavailable: 'system_authentication_unavailable',
  invalidContext: 'authorization_interrupted',
  nativeFailure: 'system_authentication_unavailable',
};
function valid(value: unknown): value is NativeIdentityResult {
  if (!value || typeof value !== 'object' || !('outcome' in value) || !('nativeCode' in value))
    return false;
  return (
    (value.outcome === 'approved' ||
      (typeof value.outcome === 'string' && Object.hasOwn(codes, value.outcome))) &&
    Number.isSafeInteger(value.nativeCode)
  );
}

/** Owns one verification attempt; native outcomes and deadlines never become a generic user cancel. */
export class IdentityVerifier {
  private pending: (() => void) | null = null;
  constructor(
    private readonly options: {
      bridge(): NativeIdentityBridge;
      visible(): boolean;
      supported(): boolean;
      log(value: Record<string, unknown>): void;
      timeoutMs?: number;
    },
  ) {}
  verify(reason: string, purpose: IdentityPurpose): Promise<boolean> {
    const verificationId = randomUUID(),
      started = Date.now();
    const trace = (phase: string, details: Record<string, unknown> = {}) => {
      try {
        this.options.log({
          application: 'vigilshell',
          timestamp: new Date().toISOString(),
          stage: 'identity_verification',
          verificationId,
          ...purpose,
          phase,
          elapsedMs: Date.now() - started,
          ...details,
        });
      } catch {
        /* A missing diagnostic sink must not change an authentication result. */
      }
    };
    const blocked = this.pending
      ? 'authorization_busy'
      : !this.options.supported()
        ? 'system_authentication_unavailable'
        : !this.options.visible()
          ? 'authorization_window_hidden'
          : null;
    if (blocked) {
      trace('not_started', { code: blocked });
      return Promise.reject(Error(blocked));
    }
    return new Promise((resolve, reject) => {
      let settled = false,
        bridge: NativeIdentityBridge | undefined,
        cancelling = false;
      const finish = (code: string | null, native?: NativeIdentityResult) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.pending = null;
        trace('settled', {
          outcome: code ? 'rejected' : 'approved',
          code,
          ...(native ? { nativeOutcome: native.outcome, nativeCode: native.nativeCode } : {}),
        });
        if (code) reject(Error(code));
        else resolve(true);
      };
      const accept = (result: unknown) => {
        if (!valid(result)) {
          finish('system_authentication_unavailable');
          return;
        }
        trace(settled || cancelling ? 'late_native_reply' : 'native_reply', {
          nativeOutcome: result.outcome,
          nativeCode: result.nativeCode,
        });
        if (!settled && !cancelling)
          finish(result.outcome === 'approved' ? null : codes[result.outcome], result);
      };
      const timer = setTimeout(() => {
        // A native result may already be queued while the JS loop reaches the deadline.
        // The bridge returns that exact result; a pending authentication is still cancelled.
        cancelling = true;
        let completed: unknown;
        try {
          completed = bridge?.cancel();
        } catch {
          trace('native_cancel_failed');
        }
        cancelling = false;
        if (valid(completed)) accept(completed);
        else finish('authorization_timed_out');
      }, this.options.timeoutMs ?? 120_000);
      this.pending = () => {
        finish('authorization_interrupted');
        try {
          bridge?.cancel();
        } catch {
          trace('native_cancel_failed');
        }
      };
      trace('started');
      try {
        bridge = this.options.bridge();
        bridge.verify(reason, accept);
      } catch {
        finish('system_authentication_unavailable');
      }
    });
  }
  cancel(): void {
    this.pending?.();
  }
}
