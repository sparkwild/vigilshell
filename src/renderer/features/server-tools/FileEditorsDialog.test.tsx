// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { FileEditorRequest, FileEditorView } from '@contracts';
import { translate } from '../../../shared/i18n/messages';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
import { FileEditorsStore } from './file-editors-store';
import { FileEditorsProvider } from './FileEditorsProvider';
import { FileEditorsDialog } from './FileEditorsDialog';
const f = vi.hoisted(() => ({
  request: vi.fn(),
  confirm: vi.fn(),
  show: () => {},
  phase: 'conflict',
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: Parameters<typeof translate>[0]) => translate(key, 'en') }),
}));
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({ state: { sessions: [] } }),
}));
vi.mock('../../shared/ConfirmationProvider', () => ({ useConfirmation: () => f.confirm }));
const item = (): FileEditorView => ({
  id: 'edit',
  connectionId: 'connection',
  path: '/srv/config',
  application: 'Editor',
  phase: f.phase,
  error: null,
});
let editorStore: FileEditorsStore;
beforeEach(() => {
  f.phase = 'conflict';
  f.confirm.mockReset().mockResolvedValue(true);
  f.request
    .mockReset()
    .mockImplementation(async () => ({ ok: true, sessions: [item()], error: null }));
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      fileEditors: {
        request: f.request,
        onChanged: () => () => {},
        onShow: (listener: () => void) => {
          f.show = listener;
          return () => {};
        },
      },
    },
  });
  editorStore = new FileEditorsStore(window.vigilshell.fileEditors);
  editorStore.start();
});
afterEach(() => {
  cleanup();
  editorStore.dispose();
});
async function open() {
  render(
    <FileEditorsProvider store={editorStore}>
      <FileEditorsDialog />
    </FileEditorsProvider>,
  );
  act(() => f.show());
  await screen.findByText('/srv/config');
  return userEvent.setup();
}
it('retains the save-as draft on outside click and asks before discarding on Escape', async () => {
  const user = await open();
  await user.click(screen.getByRole('button', { name: 'Save remote as' }));
  const draft = screen.getByRole('dialog', { name: 'Save remote as' }),
    input = within(draft).getByRole('textbox');
  await user.clear(input);
  await user.type(input, '/srv/new-config');
  await user.click(document.querySelectorAll('[data-slot="dialog-overlay"]').item(1));
  expect(input).toHaveValue('/srv/new-config');
  expect(f.confirm).not.toHaveBeenCalled();
  f.confirm.mockResolvedValue(false);
  await user.keyboard('{Escape}');
  await waitFor(() => expect(f.confirm).toHaveBeenCalledOnce());
  expect(input).toHaveValue('/srv/new-config');
  f.confirm.mockResolvedValue(true);
  await user.click(within(draft).getByRole('button', { name: 'Cancel' }));
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: 'Save remote as' })).not.toBeInTheDocument(),
  );
});
it('shows save-as failure inside the active form and preserves its path', async () => {
  const user = await open();
  f.request.mockImplementation(async (request: FileEditorRequest) => ({
    ok: request.action !== 'resolve',
    sessions: [item()],
    error: request.action === 'resolve' ? 'files_exists' : null,
  }));
  await user.click(screen.getByRole('button', { name: 'Save remote as' }));
  const draft = screen.getByRole('dialog', { name: 'Save remote as' }),
    input = within(draft).getByRole('textbox');
  await user.clear(input);
  await user.type(input, '/srv/existing');
  await user.keyboard('{Enter}');
  await waitFor(() => expect(input).toHaveAttribute('aria-invalid', 'true'));
  expect(
    within(draft).getByText(translate(userErrorKey('files_exists'), 'en')),
  ).toBeInTheDocument();
  expect(input).toHaveValue('/srv/existing');
});
it('describes remote replacement accurately before overwriting', async () => {
  const user = await open();
  await user.click(screen.getByRole('button', { name: 'Overwrite verified remote version' }));
  expect(f.confirm).toHaveBeenCalledWith(
    expect.objectContaining({ description: translate('file_editor_overwrite_body', 'en') }),
  );
  await waitFor(() =>
    expect(f.request).toHaveBeenCalledWith({
      action: 'resolve',
      id: 'edit',
      choice: 'overwrite',
      destination: null,
    }),
  );
});
it('locks end-editing requests and displays IPC rejection without an unhandled promise', async () => {
  f.phase = 'synced';
  const user = await open();
  let reject!: (reason: Error) => void;
  f.request.mockImplementation((request: FileEditorRequest) =>
    request.action === 'end'
      ? new Promise((_resolve, fail) => {
          reject = fail;
        })
      : Promise.resolve({ ok: true, sessions: [item()], error: null }),
  );
  await user.dblClick(screen.getByRole('button', { name: 'End editing' }));
  expect(f.request.mock.calls.filter(([request]) => request.action === 'end')).toHaveLength(1);
  expect(screen.getByRole('button', { name: 'End editing' })).toBeDisabled();
  await act(async () => reject(new Error('IPC interrupted')));
  expect(
    await screen.findByText(translate(userErrorKey('runtime_unavailable'), 'en')),
  ).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'End editing' })).toBeEnabled();
});
it('confirms unsynced content before repeating end with explicit discard', async () => {
  f.phase = 'failed';
  const user = await open();
  f.request.mockImplementation(async (request: FileEditorRequest) => ({
    ok: request.action !== 'end' || request.discard,
    sessions: [item()],
    error: request.action === 'end' && !request.discard ? 'file_editor_unsynced' : null,
  }));
  await user.click(screen.getByRole('button', { name: 'End editing' }));
  await waitFor(() =>
    expect(f.request).toHaveBeenCalledWith({ action: 'end', id: 'edit', discard: true }),
  );
  expect(f.confirm).toHaveBeenCalledWith(
    expect.objectContaining({ description: translate('file_editor_discard_body', 'en') }),
  );
});
it('guards an edited save-as path before notification navigation and closes only after acceptance', async () => {
  let leave: (() => Promise<boolean>) | null = null;
  const register = (guard: typeof leave) => {
    leave = guard;
  };
  const view = render(
    <FileEditorsProvider store={editorStore}>
      <FileEditorsDialog registerLeaveGuard={register} />
    </FileEditorsProvider>,
  );
  act(() => f.show());
  await screen.findByText('/srv/config');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: 'Save remote as' }));
  const draft = screen.getByRole('dialog', { name: 'Save remote as' });
  const input = within(draft).getByRole('textbox');
  await user.clear(input);
  await user.type(input, '/srv/keep-my-draft');
  f.confirm.mockResolvedValue(false);
  let accepted = true;
  await act(async () => {
    accepted = await leave!();
  });
  expect(accepted).toBe(false);
  expect(input).toHaveValue('/srv/keep-my-draft');
  expect(f.request.mock.calls.some(([request]) => request.action === 'resolve')).toBe(false);
  f.confirm.mockResolvedValue(true);
  await act(async () => {
    accepted = await leave!();
  });
  expect(accepted).toBe(true);
  view.rerender(
    <FileEditorsProvider store={editorStore}>
      <FileEditorsDialog registerLeaveGuard={register} navigationVersion={1} />
    </FileEditorsProvider>,
  );
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: 'Editing files' })).not.toBeInTheDocument(),
  );
});
