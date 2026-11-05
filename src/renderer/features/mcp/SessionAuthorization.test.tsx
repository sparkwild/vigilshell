// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { StrictMode } from 'react';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { SessionAuthorization } from './SessionAuthorization';
const state = vi.hoisted(() => ({
  runtimeId: 'runtime',
  ready: true,
  focused: true,
  contexts: [] as { id: string; phase: string; createdAt: number }[],
  issue: vi.fn(),
}));
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({
    state: { runtimeId: state.runtimeId, transportReady: state.ready },
  }),
}));
vi.mock('./model', () => ({
  useMcp: () => ({
    snapshot: { contexts: state.contexts },
    issue: state.issue,
  }),
}));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const context = (id: string, createdAt = 1) => ({ id, phase: 'waitingUser', createdAt });
beforeEach(() => {
  state.runtimeId = 'runtime';
  state.ready = true;
  state.focused = true;
  state.contexts = [context('first')];
  state.issue.mockReset().mockResolvedValue(false);
  vi.spyOn(document, 'hasFocus').mockImplementation(() => state.focused);
});
it('starts system verification once in StrictMode without adding an application dialog', async () => {
  const view = render(
    <StrictMode>
      <SessionAuthorization />
    </StrictMode>,
  );
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(1));
  expect(state.issue).toHaveBeenCalledWith({ action: 'approveConnection', contextId: 'first' });
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
  state.contexts = [...state.contexts];
  view.rerender(
    <StrictMode>
      <SessionAuthorization />
    </StrictMode>,
  );
  await act(async () => {});
  expect(state.issue).toHaveBeenCalledTimes(1);
});
it('defers background requests until foreground and does not retry cancellation on refocus', async () => {
  state.focused = false;
  render(<SessionAuthorization />);
  expect(state.issue).not.toHaveBeenCalled();
  await act(async () => {
    state.focused = true;
    window.dispatchEvent(new Event('focus'));
  });
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(1));
  await act(async () => {
    state.focused = false;
    window.dispatchEvent(new Event('blur'));
  });
  await act(async () => {
    state.focused = true;
    window.dispatchEvent(new Event('focus'));
  });
  expect(state.issue).toHaveBeenCalledTimes(1);
});
it('waits for an existing application dialog to close', async () => {
  const other = document.createElement('div');
  other.setAttribute('aria-modal', 'true');
  document.body.append(other);
  render(<SessionAuthorization />);
  expect(state.issue).not.toHaveBeenCalled();
  await act(async () => other.remove());
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(1));
});
it('serializes waiting sessions across native focus loss and snapshot updates', async () => {
  let finish!: (value: boolean) => void;
  state.contexts = [context('second', 2), context('first', 1)];
  state.issue.mockImplementationOnce(
    () =>
      new Promise<boolean>((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(<SessionAuthorization />);
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(1));
  expect(state.issue).toHaveBeenLastCalledWith({ action: 'approveConnection', contextId: 'first' });
  await act(async () => {
    state.focused = false;
    window.dispatchEvent(new Event('blur'));
  });
  state.contexts = [...state.contexts];
  view.rerender(<SessionAuthorization />);
  expect(state.issue).toHaveBeenCalledTimes(1);
  await act(async () => finish(true));
  expect(state.issue).toHaveBeenCalledTimes(1);
  await act(async () => {
    state.focused = true;
    window.dispatchEvent(new Event('focus'));
  });
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(2));
  expect(state.issue).toHaveBeenLastCalledWith({
    action: 'approveConnection',
    contextId: 'second',
  });
});
it('drops a request that ends while queued and does not start during disconnection', async () => {
  state.focused = false;
  const view = render(<SessionAuthorization />);
  state.contexts = [{ ...context('first'), phase: 'closed' }];
  view.rerender(<SessionAuthorization />);
  await act(async () => {
    state.focused = true;
    window.dispatchEvent(new Event('focus'));
  });
  expect(state.issue).not.toHaveBeenCalled();
  state.ready = false;
  state.contexts = [context('pending')];
  view.rerender(<SessionAuthorization />);
  expect(state.issue).not.toHaveBeenCalled();
  state.ready = true;
  view.rerender(<SessionAuthorization />);
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(1));
});
it('waits for a late old-runtime attempt before processing a new runtime', async () => {
  let finish!: (value: boolean) => void;
  state.issue.mockImplementationOnce(
    () =>
      new Promise<boolean>((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(<SessionAuthorization />);
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(1));
  state.ready = false;
  view.rerender(<SessionAuthorization />);
  state.runtimeId = 'new-runtime';
  state.contexts = [context('new')];
  state.ready = true;
  view.rerender(<SessionAuthorization />);
  expect(state.issue).toHaveBeenCalledTimes(1);
  await act(async () => finish(false));
  await waitFor(() => expect(state.issue).toHaveBeenCalledTimes(2));
  expect(state.issue).toHaveBeenLastCalledWith({ action: 'approveConnection', contextId: 'new' });
});
