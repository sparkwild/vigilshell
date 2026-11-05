// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { ConnectionSummary } from '@contracts';
import type { McpUiResult } from '../../../../contracts/mcp';
import { usePolicyBrowser } from './use-policy-browser';
afterEach(cleanup);
const connection = { id: 'c', revision: 1 } as ConnectionSummary;
function deferred() {
  let resolve!: (value: McpUiResult) => void;
  const promise = new Promise<McpUiResult>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
it('closes a lease whose handshake arrives after the picker is dismissed', async () => {
  const opening = deferred();
  const request = vi
    .fn()
    .mockImplementation((op) =>
      op.action === 'openPolicyBrowser' ? opening.promise : Promise.resolve({ ok: true }),
    );
  Object.defineProperty(window, 'vigilshell', { configurable: true, value: { mcp: { request } } });
  const { result, unmount } = renderHook(() => usePolicyBrowser(connection));
  let read!: ReturnType<typeof result.current.read>;
  act(() => {
    read = result.current.read({ kind: 'directory', path: '/srv', after: null });
  });
  unmount();
  opening.resolve({ ok: true, reply: { kind: 'policyBrowser', id: 'late' } });
  expect(await read).toBeNull();
  await Promise.resolve();
  expect(request.mock.calls.map(([v]) => v.action)).toEqual([
    'openPolicyBrowser',
    'closePolicyBrowser',
  ]);
  expect(request).toHaveBeenLastCalledWith({ action: 'closePolicyBrowser', browserId: 'late' });
});
it('reuses the inspection lease and never opens an Agent session or creates a task', async () => {
  const request = vi.fn().mockImplementation((op) =>
    Promise.resolve(
      op.action === 'openPolicyBrowser'
        ? { ok: true, reply: { kind: 'policyBrowser', id: 'browser' } }
        : op.action === 'readPolicyResources'
          ? {
              ok: true,
              reply: {
                kind: 'policyResources',
                page: { kind: 'directory', path: '/srv', page: { entries: [], nextCursor: null } },
              },
            }
          : { ok: true },
    ),
  );
  Object.defineProperty(window, 'vigilshell', { configurable: true, value: { mcp: { request } } });
  const { result, unmount } = renderHook(() => usePolicyBrowser(connection));
  await act(async () => {
    await result.current.read({ kind: 'directory', path: '/srv', after: null });
    await result.current.read({ kind: 'directory', path: '/', after: null });
  });
  expect(request.mock.calls.map(([v]) => v.action)).toEqual([
    'openPolicyBrowser',
    'readPolicyResources',
    'readPolicyResources',
  ]);
  unmount();
  await Promise.resolve();
  expect(request).toHaveBeenLastCalledWith({ action: 'closePolicyBrowser', browserId: 'browser' });
});

it('keeps the shared lease after one directory is inaccessible', async () => {
  const request = vi.fn().mockImplementation(async (op) =>
    op.action === 'openPolicyBrowser'
      ? { ok: true, reply: { kind: 'policyBrowser', id: 'browser' } }
      : op.action === 'readPolicyResources' && op.query.path === '/private'
        ? { ok: false, code: 'files_permission_denied' }
        : {
            ok: true,
            reply: {
              kind: 'policyResources',
              page: { kind: 'directory', path: '/', page: { entries: [], nextCursor: null } },
            },
          },
  );
  Object.defineProperty(window, 'vigilshell', { configurable: true, value: { mcp: { request } } });
  const { result, unmount } = renderHook(() => usePolicyBrowser(connection));
  await act(async () => {
    await result.current.read({ kind: 'directory', path: '/private', after: null });
    await result.current.read({ kind: 'directory', path: '/', after: null });
  });
  expect(request.mock.calls.map(([op]) => op.action)).toEqual([
    'openPolicyBrowser',
    'readPolicyResources',
    'readPolicyResources',
  ]);
  unmount();
});
