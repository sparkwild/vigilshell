// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ConnectionDetails, WorkspaceOperation, WorkspaceReply } from '@contracts';
import { translate } from '../../../../shared/i18n/messages';
import { OperationStore, type OperationRequest } from '../operations';
import { WorkspaceFailure } from '../workspace-failure';
import type { Editor, WorkspaceStore } from '../workspace-store';
import { useConnectionEditor } from './use-connection-editor';

let fixture: ReturnType<typeof setup>;
vi.mock('../../../app/DesktopProvider', () => {
  const t = (key: Parameters<typeof translate>[0]) => translate(key, 'en');
  return { useDesktop: () => ({ t }) };
});
vi.mock('../../../shared/ConfirmationProvider', () => ({ useConfirmation: () => fixture.confirm }));
vi.mock('../ui-context', () => ({ useWorkspaceUi: () => fixture.ui }));
vi.mock('../workspace-context', async (original) => ({
  ...(await original<object>()),
  useWorkspace: () => ({ store: fixture.store, state: fixture.state }),
}));

function setup() {
  const editor: Editor = { kind: 'connection', token: 'original', connection: null, groupId: null };
  const state = { runtimeId: 'runtime', ready: true, editor: editor as Editor | null };
  const feedback = new OperationStore({
    get: vi.fn(),
    cancel: vi.fn(),
    reconcile: vi.fn(),
    runtimeId: () => 'runtime',
  });
  const connection = {
    id: 'connection',
    version: 1,
    credentialSaved: false,
    config: { name: 'Fixture', host: 'localhost', port: 22, username: 'fixture', auth: 'password' },
  } as ConnectionDetails;
  const job: WorkspaceOperation = {
    id: 'saved',
    kind: 'save',
    phase: 'success',
    stage: 'save',
    error: null,
    connection,
    proxy: null,
    draft: null,
    challenge: null,
    sessionId: null,
  };
  const mutation = vi
    .fn<(...args: unknown[]) => Promise<WorkspaceReply>>()
    .mockResolvedValue({ kind: 'operation', operation: job });
  const closeEditor = vi.fn(),
    select = vi.fn(),
    connect = vi.fn();
  const store = {
    feedback,
    mutation,
    getSnapshot: () => state,
    run: (request: OperationRequest) => feedback.run(request),
    settled: async (reply: WorkspaceReply) => (reply.kind === 'operation' ? reply.operation : null),
    call: vi.fn(),
    closeEditor,
    select,
    connect,
    refresh: vi.fn(),
    report: vi.fn(),
  } as unknown as WorkspaceStore;
  const ui = { active: true, setDirty: vi.fn(), setCloseEditor: vi.fn() };
  return {
    editor,
    state,
    store,
    feedback,
    mutation,
    closeEditor,
    select,
    connect,
    job,
    ui,
    confirm: vi.fn().mockResolvedValue(true),
  };
}
beforeEach(() => {
  fixture = setup();
});
afterEach(() => {
  cleanup();
  fixture.feedback.dispose();
});

function mount() {
  return renderHook(() => useConnectionEditor({ editor: fixture.editor }));
}

it('keeps all section drafts and saves normalized metadata, terminal, proxy and tunnel fields', async () => {
  const { result } = mount();
  const proxyId = '00000000-0000-4000-8000-000000000001';
  await act(async () => {
    const form = result.current.form;
    form.setValue('host', 'localhost');
    form.setValue('username', 'fixture');
    form.setValue('notes', '中文备注\n保留换行');
    form.setValue('terminal', { encoding: 'gbk', backspace: 'bs', delete: 'del' });
    form.setValue('proxyId', proxyId);
    form.setValue('tunnels', [
      {
        id: crypto.randomUUID(),
        name: 'SOCKS',
        kind: 'socks5',
        bindHost: '127.0.0.1',
        listenPort: 18081,
        targetHost: null,
        targetPort: null,
        enabled: true,
      },
    ]);
    result.current.setSection('terminal');
  });
  await act(async () => {
    result.current.setSection('proxy');
  });
  await act(async () => {
    result.current.setSection('tunnels');
  });
  await act(async () => {
    await result.current.form.handleSubmit((values) => result.current.save(values, false))();
  });
  expect(fixture.mutation).toHaveBeenCalledTimes(1);
  expect(fixture.mutation).toHaveBeenCalledWith(
    'createConnection',
    expect.objectContaining({
      draftId: null,
      config: expect.objectContaining({
        name: 'localhost',
        notes: '中文备注\n保留换行',
        proxyId,
        terminal: { encoding: 'gbk', backspace: 'bs', delete: 'del' },
        tunnels: [expect.objectContaining({ kind: 'socks5', targetHost: null, targetPort: null })],
      }),
    }),
  );
  expect(fixture.closeEditor).toHaveBeenCalledOnce();
  expect(fixture.connect).not.toHaveBeenCalled();
});

it('retains unsaved values and the original request handle when a save result is unknown', async () => {
  fixture.mutation.mockRejectedValue(new WorkspaceFailure('result_unknown', 'request', 'runtime'));
  const { result } = mount();
  await act(async () => {
    result.current.form.setValue('name', 'Unsaved');
    await result.current.save(result.current.form.getValues(), false);
  });
  expect(result.current.form.getValues('name')).toBe('Unsaved');
  expect(result.current.unknown).toMatchObject({ requestId: 'request', runtimeId: 'runtime' });
  expect(fixture.feedback.isBusy('editor:original')).toBe(true);
  expect(fixture.closeEditor).not.toHaveBeenCalled();
  expect(fixture.mutation).toHaveBeenCalledOnce();
});

it('does not close or select over a newer editor after an older save completes', async () => {
  let finish!: (reply: WorkspaceReply) => void;
  fixture.mutation.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const { result } = mount();
  let work!: Promise<void>;
  await act(async () => {
    work = result.current.save(result.current.form.getValues(), false);
  });
  fixture.state.editor = { ...fixture.editor, token: 'newer' };
  await act(async () => {
    finish({ kind: 'operation', operation: fixture.job });
    await work;
  });
  expect(fixture.closeEditor).not.toHaveBeenCalled();
  expect(fixture.select).not.toHaveBeenCalled();
  expect(fixture.state.editor.token).toBe('newer');
});

it('rejects save-and-connect without credentials and allows discarding an unsaved draft', async () => {
  const { result } = mount();
  await act(async () => {
    await result.current.save(result.current.form.getValues(), true);
  });
  expect(result.current.error).toBe('credential_missing');
  expect(fixture.mutation).not.toHaveBeenCalled();
  await act(async () => {
    result.current.form.setValue('name', 'Discard me', { shouldDirty: true });
  });
  await act(async () => {
    await result.current.close();
  });
  expect(fixture.confirm).toHaveBeenCalledOnce();
  expect(fixture.closeEditor).toHaveBeenCalledOnce();
  expect(fixture.mutation).not.toHaveBeenCalled();
});
