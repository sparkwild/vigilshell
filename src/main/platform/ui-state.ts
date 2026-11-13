import { EventEmitter } from 'node:events';
import type { UiStateData, UiStateRecord } from '@contracts';
import {
  uiScopeSchema,
  uiStateDataSchema,
  type UiStateNotice,
  type UiStatePatch,
} from '../../shared/ui-state';
import { BackendClient, BackendError } from '../backend/client';
type Entry = {
  record: UiStateRecord;
  desired: UiStateData | null;
  dirty: boolean;
  error: string | null;
  running?: Promise<void>;
  timer?: ReturnType<typeof setTimeout>;
  unresolved?: { id: string; runtimeId: string };
};
/** 由 Main 串行合并各作用域更新；不占用 Preferences 的保存锁。 */
export class UiStateService extends EventEmitter<{ changed: [UiStateNotice] }> {
  private entries = new Map<string, Entry>();
  private reads = new Map<string, Promise<Entry>>();
  constructor(private host: BackendClient) {
    super();
    host.on('unavailable', () => {
      for (const e of this.entries.values()) {
        e.error = 'runtime_unavailable';
      }
    });
  }
  private notice(e: Entry): UiStateNotice {
    return { scope: e.record.scope, data: e.desired, error: e.error };
  }
  private async entry(scope: string): Promise<Entry> {
    uiScopeSchema.parse(scope);
    const found = this.entries.get(scope);
    if (found) return found;
    const reading = this.reads.get(scope);
    if (reading) return reading;
    const work = (async () => {
      const reply = await this.host.request({
        method: 'uiState.get',
        params: { scope },
      });
      if (reply.kind !== 'uiState') throw Error('protocol_mismatch');
      const e: Entry = {
        record: reply.state,
        desired: reply.state.data,
        dirty: false,
        error: null,
      };
      this.entries.set(scope, e);
      return e;
    })().finally(() => this.reads.delete(scope));
    this.reads.set(scope, work);
    return work;
  }
  async get(scope: string): Promise<UiStateNotice> {
    return this.notice(await this.entry(scope));
  }
  async set(scope: string, patch: UiStatePatch, immediate = false): Promise<UiStateNotice> {
    const e = await this.entry(scope);
    const fallback = scope === 'pickers' ? { kind: 'pickers', upload: '', download: '' } : {};
    const data = uiStateDataSchema.parse({
      ...fallback,
      ...e.desired,
      ...patch,
    });
    if (data.kind === 'directory' ? !scope.startsWith('connection:') : scope !== data.kind)
      throw Error('invalid_request');
    e.desired = data;
    e.dirty = true;
    clearTimeout(e.timer);
    if (immediate) await this.drain(e);
    else e.timer = setTimeout(() => void this.drain(e), 500);
    return this.notice(e);
  }
  private async reconcile(e: Entry) {
    const pending = e.unresolved!;
    const r = await this.host.request({
      method: 'uiState.reconcile',
      params: {
        scope: e.record.scope,
        requestId: pending.id,
        runtimeId: pending.runtimeId,
      },
    });
    if (r.kind !== 'uiReconciled') throw Error('protocol_mismatch');
    this.host.release(pending.id);
    e.unresolved = undefined;
    e.record = r.state;
    if (!r.outcome) throw Error('result_unknown');
    if (r.outcome.error) throw Error(r.outcome.error.data.code);
  }
  private drain(e: Entry): Promise<void> {
    if (e.running) return e.running;
    e.running = (async () => {
      try {
        if (e.unresolved) await this.reconcile(e);
        // A FIFO read also refreshes revisions after a Host restart.
        const current = await this.host.request({
          method: 'uiState.get',
          params: { scope: e.record.scope },
        });
        if (current.kind !== 'uiState') throw Error('protocol_mismatch');
        e.record = current.state;
        while (e.dirty && e.desired) {
          const desired = e.desired;
          e.dirty = false;
          const call = this.host.call({
            method: 'uiState.update',
            params: {
              scope: e.record.scope,
              expectedRevision: e.record.revision,
              data: desired,
            },
          });
          e.unresolved = { id: call.id, runtimeId: this.host.instanceId! };
          try {
            const reply = await call.result;
            if (reply.kind !== 'uiState') throw Error('protocol_mismatch');
            e.record = reply.state;
            e.unresolved = undefined;
          } catch (error) {
            if (
              error instanceof BackendError &&
              ['timeout', 'runtime_unavailable'].includes(error.code)
            )
              await this.reconcile(e);
            else {
              e.unresolved = undefined;
              throw error;
            }
          }
          e.error = null;
          if (!e.dirty) e.desired = e.record.data;
          this.emit('changed', this.notice(e));
        }
        if (!e.dirty && !e.unresolved) {
          e.error = null;
          e.desired = e.record.data;
        }
      } catch (error) {
        e.dirty = true;
        e.error =
          error instanceof BackendError
            ? error.code
            : error instanceof Error
              ? error.message
              : 'storage_unavailable';
        this.emit('changed', this.notice(e));
      }
    })().finally(() => {
      e.running = undefined;
    });
    return e.running;
  }
  async retry(scope: string) {
    const e = await this.entry(scope);
    await this.drain(e);
    return this.notice(e);
  }
  async flush(requireSaved = false) {
    for (const e of this.entries.values()) {
      clearTimeout(e.timer);
      if (e.dirty || e.running || (requireSaved && (e.error || e.unresolved))) await this.drain(e);
    }
    if (requireSaved && [...this.entries.values()].some((e) => e.dirty || e.unresolved || e.error))
      throw Error('ui_state_not_saved');
  }
}
