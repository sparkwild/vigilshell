// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { translate } from '../../../shared/i18n/messages';
import { FileOperationsDialog } from './FileOperationsDialog';
const f = vi.hoisted(() => ({ call: vi.fn(), mutation: vi.fn(), phase: 'failed' }));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], params?: Record<string, string | number>) =>
      translate(key, 'en', params),
  }),
}));
vi.mock('../connections/workspace-context', () => {
  const state = {
    runtimeId: 'runtime',
    operations: new Map([['job', { id: 'job', kind: 'files', sessionId: 'session' }]]),
  };
  const store = {
    call: f.call,
    mutation: f.mutation,
    getSnapshot: () => ({ runtimeId: 'runtime' }),
  };
  return {
    useWorkspace: () => ({ state, store }),
    errorCode: () => 'tools_unavailable',
    errorKey: () => 'tools_unavailable',
  };
});
beforeEach(() => {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  f.phase = 'failed';
  f.mutation.mockReset().mockResolvedValue({ kind: 'operation', operation: { id: 'job' } });
  f.call.mockReset().mockImplementation(async (method, params) =>
    method === 'getFileOperation'
      ? {
          kind: 'fileOperation',
          page: {
            id: 'job',
            phase: f.phase,
            stage: 'finished',
            completed: 1,
            failed: 1,
            skipped: 0,
            pending: 0,
            error: null,
            items: [
              {
                entry: { path: params.offset ? '/failed' : '/complete' },
                status: params.offset ? 'failed' : 'complete',
                error: null,
              },
            ],
            next: params.offset ? null : 1,
          },
        }
      : { kind: 'done' },
  );
});
afterEach(cleanup);
it('pages operation results and retries the existing job without submitting its successful targets again', async () => {
  render(<FileOperationsDialog sessionId="session" initialId="job" close={vi.fn()} />);
  const user = userEvent.setup();
  await screen.findByText('/complete');
  await user.click(screen.getByRole('button', { name: 'Load more details' }));
  await screen.findByText('/failed');
  expect(f.call).toHaveBeenCalledWith('getFileOperation', { id: 'job', offset: 1 });
  await user.click(screen.getByRole('button', { name: 'Retry failed and pending items' }));
  expect(f.mutation).toHaveBeenCalledWith('retryFileOperation', { id: 'job' });
  expect(f.mutation).toHaveBeenCalledOnce();
});
it('closing an active operation dialog does not cancel its background job', async () => {
  f.phase = 'working';
  const close = vi.fn();
  render(<FileOperationsDialog sessionId="session" initialId="job" close={close} />);
  const user = userEvent.setup();
  await screen.findByText('/complete');
  await user.click(screen.getAllByRole('button', { name: 'Close' }).at(-1)!);
  expect(close).toHaveBeenCalledOnce();
  expect(f.call.mock.calls.some(([method]) => method === 'cancelOperation')).toBe(false);
  await user.click(screen.getByRole('button', { name: 'Cancel' }));
  await waitFor(() => expect(f.call).toHaveBeenCalledWith('cancelOperation', { id: 'job' }));
});
