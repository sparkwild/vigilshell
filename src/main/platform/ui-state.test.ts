import { EventEmitter } from 'node:events';
import { describe, it, expect, vi } from 'vitest';
import type { RpcCommand, RpcValue, UiStateRecord, UiStateData } from '@contracts';
import { UiStateService } from './ui-state';
import { BackendError, type BackendClient } from '../backend/client';
class FakeHost extends EventEmitter {
  instanceId = 'runtime';
  records = new Map<string, UiStateRecord>();
  sequence = 0;
  timeout = false;
  unknown = false;
  release = vi.fn();
  request = vi.fn(async (command: RpcCommand): Promise<RpcValue> => {
    if (command.method === 'uiState.get' || command.method === 'uiState.reconcile') {
      const state = this.records.get(command.params.scope) ?? {
        scope: command.params.scope,
        revision: 0,
        data: null,
      };
      return command.method === 'uiState.get'
        ? { kind: 'uiState', state }
        : {
            kind: 'uiReconciled',
            state,
            outcome: this.unknown
              ? null
              : {
                  status: 'applied',
                  uiState: state,
                  preferences: null,
                  error: null,
                },
          };
    }
    throw Error('unexpected request');
  });
  call = vi.fn((command: RpcCommand) => {
    if (command.method !== 'uiState.update') throw Error('unexpected mutation');
    const p = command.params;
    const state = {
      scope: p.scope,
      revision: p.expectedRevision + 1,
      data: p.data,
    };
    this.records.set(p.scope, state);
    return {
      id: String(++this.sequence),
      result: this.timeout
        ? Promise.reject(new BackendError('timeout'))
        : Promise.resolve({ kind: 'uiState', state }),
    };
  });
}

const pickers: Extract<UiStateData, { kind: 'pickers' }> = {
  kind: 'pickers',
  upload: '',
  download: '',
};
const connectionScope = 'connection:00000000-0000-0000-0000-000000000001';
describe('browsing state persistence', () => {
  it('merges browsing changes per scope without writing preferences', async () => {
    const host = new FakeHost(),
      service = new UiStateService(host as unknown as BackendClient);
    await service.set('pickers', { ...pickers, upload: '/uploads' });
    await service.set('pickers', { kind: 'pickers', download: '/downloads' });
    await service.set(connectionScope, {
      kind: 'directory',
      connectionRevision: 1,
      path: '/remote',
      scroll: 0,
      expanded: [],
    });
    await service.flush();
    expect(host.records.get('pickers')?.data).toMatchObject({
      upload: '/uploads',
      download: '/downloads',
    });
    expect(host.records.get(connectionScope)?.data).toMatchObject({
      path: '/remote',
    });
    expect(host.call).toHaveBeenCalledTimes(2);
    expect(host.call.mock.calls.every(([c]) => c.method === 'uiState.update')).toBe(true);
  });
  it('reconciles a timed out write without replaying it', async () => {
    const host = new FakeHost(),
      service = new UiStateService(host as unknown as BackendClient);
    host.timeout = true;
    const result = await service.set('pickers', pickers, true);
    expect(result.error).toBeNull();
    expect(host.call).toHaveBeenCalledTimes(1);
    expect(host.request.mock.calls.some(([c]) => c.method === 'uiState.reconcile')).toBe(true);
    expect(host.release).toHaveBeenCalledWith('1');
  });
  it('retains an unknown outcome without claiming success', async () => {
    const host = new FakeHost(),
      service = new UiStateService(host as unknown as BackendClient);
    host.timeout = true;
    host.unknown = true;
    const result = await service.set('pickers', { ...pickers, download: '/downloads' }, true);
    expect(result.error).toBe('result_unknown');
    expect(result.data).toMatchObject({ download: '/downloads' });
    expect(host.call).toHaveBeenCalledTimes(1);
  });
  it('rejects removed layout scopes and mismatched browsing data', async () => {
    const host = new FakeHost(),
      service = new UiStateService(host as unknown as BackendClient);
    for (const scope of ['window', 'workbench', 'files'])
      await expect(service.set(scope, pickers, true)).rejects.toThrow();
    await expect(service.set(connectionScope, pickers, true)).rejects.toThrow();
    expect(host.call).not.toHaveBeenCalled();
  });
});
