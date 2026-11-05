// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { AgentSnapshot, AgentUiRequest } from '@contracts';
import { useMcp } from './model';

afterEach(cleanup);
const actions: AgentUiRequest[] = [
  { action: 'approveConnection', contextId: 'session' },
  { action: 'approveScript', taskId: 'script' },
  {
    action: 'setPolicyAccess',
    connectionId: 'connection',
    connectionRevision: 1,
    expectedRevision: 1,
    enabled: true,
  },
];
it.each(actions)('keeps the confirmed snapshot after declined verification: %j', async (action) => {
  const snapshot = {
    policies: [],
    settings: { showTray: true, retentionDays: 30 },
  } as unknown as AgentSnapshot;
  let code = 'authorization_cancelled';
  const request = vi.fn(async (r: AgentUiRequest) =>
    r.action === 'snapshot' ? { ok: true, snapshot } : { ok: false, code },
  );
  Object.assign(window, { vigilshell: { mcp: { request, onChanged: () => () => {} } } });
  const { result } = renderHook(useMcp);
  await waitFor(() => expect(result.current.snapshot).toBe(snapshot));
  for (const failure of [
    'authorization_cancelled',
    'authorization_failed',
    'authorization_timed_out',
    'authorization_interrupted',
  ]) {
    code = failure;
    await act(async () => {
      expect(await result.current.issue(action)).toBe(false);
    });
    expect(result.current.error).toBeNull();
    expect(result.current.snapshot).toBe(snapshot);
  }
  expect(request).toHaveBeenCalledTimes(5);
  code = 'configuration_conflict';
  await act(async () => {
    await result.current.issue(action);
  });
  expect(result.current.error).toBe(code);
});
