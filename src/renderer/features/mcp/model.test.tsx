// @vitest-environment jsdom
import { act, renderHook, cleanup } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { AgentSnapshot } from '@contracts';
import type { McpUiResult } from '../../../contracts/mcp';
import { defaultPolicySettings, useMcp } from './model';
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
afterEach(cleanup);
it('in-flight reads from before or during a policy save cannot overwrite the confirmed snapshot', async () => {
  const before = deferred<McpUiResult>(),
    during = deferred<McpUiResult>(),
    save = deferred<McpUiResult>();
  const policy = defaultPolicySettings('one');
  const old: AgentSnapshot = {
    settings: {
      revision: 0,
      enabled: false,
      port: 51602,
      showTray: true,
      retentionDays: 30,
      storageMib: 256,
    },
    entries: [],
    policies: [policy],
    contexts: [],
    views: [],
    clients: [],
    observations: [],
    endpoint: 'http://127.0.0.1:51602/mcp',
    running: false,
    error: null,
  };
  const next = {
    ...old,
    policies: [
      { ...policy, execution: { ...policy.execution, level: 'operate' as const }, revision: 8 },
    ],
  };
  let changed!: (event: { kind: 'configuration' }) => void;
  const reads = [before, during];
  const request = vi.fn((value: { action: string }) =>
    value.action === 'snapshot' ? reads.shift()!.promise : save.promise,
  );
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      mcp: {
        request,
        onChanged: (receive: typeof changed) => {
          changed = receive;
          return () => {};
        },
      },
    },
  });
  const { result } = renderHook(() => useMcp());
  let operation!: Promise<boolean>;
  act(() => {
    operation = result.current.issue({
      action: 'updatePolicySettings',
      connectionId: 'one',
      connectionRevision: 0,
      expectedRevision: 0,
      patch: { field: 'level', value: 'operate' },
    });
  });
  act(() => changed({ kind: 'configuration' }));
  await act(async () => {
    save.resolve({ ok: true, snapshot: next });
    await operation;
  });
  expect(result.current.snapshot?.policies[0]?.revision).toBe(8);
  await act(async () => {
    before.resolve({ ok: true, snapshot: old });
    during.resolve({ ok: true, snapshot: old });
    await Promise.all([before.promise, during.promise]);
  });
  expect(result.current.snapshot?.policies[0]?.revision).toBe(8);
});
