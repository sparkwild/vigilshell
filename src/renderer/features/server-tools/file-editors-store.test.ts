import { expect, it, vi } from 'vitest';
import type { FileEditorsApi } from '../../../contracts/file-editors';
import type { FileEditorView, FileEditorReply } from '@contracts';
import { FileEditorsStore } from './file-editors-store';
const item = (id: string): FileEditorView => ({
  id,
  connectionId: 'connection',
  path: '/fixture',
  application: 'Editor',
  phase: 'conflict',
  error: null,
});
it('keeps a single application subscription without mounted dialogs and preserves confirmed state on failure', async () => {
  let changed = () => {};
  const stop = vi.fn();
  const api: FileEditorsApi = {
    request: vi.fn(async () => ({ ok: true, sessions: [item('first')], error: null })),
    onChanged: vi.fn((listener) => {
      changed = listener;
      return stop;
    }),
    onShow: () => () => {},
  };
  const store = new FileEditorsStore(api);
  store.start();
  store.start();
  await vi.waitFor(() => expect(store.getSnapshot().items[0]?.id).toBe('first'));
  expect(api.onChanged).toHaveBeenCalledOnce();
  vi.mocked(api.request).mockRejectedValueOnce(Error('unavailable'));
  changed();
  await vi.waitFor(() => expect(store.getSnapshot().error).toBe('runtime_unavailable'));
  expect(store.getSnapshot().items[0]?.id).toBe('first');
  store.dispose();
  store.dispose();
  expect(stop).toHaveBeenCalledOnce();
});
it('does not let a pending read overwrite a mutation or write after disposal', async () => {
  let resolve!: (reply: FileEditorReply) => void;
  const api: FileEditorsApi = {
    request: vi.fn(
      () =>
        new Promise<FileEditorReply>((done) => {
          resolve = done;
        }),
    ),
    onChanged: () => () => {},
    onShow: () => () => {},
  };
  const store = new FileEditorsStore(api);
  store.start();
  store.accept([item('mutation')]);
  resolve({ ok: true, sessions: [item('old')], error: null });
  await Promise.resolve();
  expect(store.getSnapshot().items[0]?.id).toBe('mutation');
  const pending = store.refresh();
  store.dispose();
  resolve({ ok: true, sessions: [item('late')], error: null });
  await pending;
  expect(store.getSnapshot().items[0]?.id).toBe('mutation');
});
