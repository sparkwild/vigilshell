import type { FileEditorView } from '@contracts';
import type { FileEditorsApi } from '../../../contracts/file-editors';
/** Owns only the renderer snapshot; Main continues to own editing sessions. */
export class FileEditorsStore {
  private snapshot = { items: [] as FileEditorView[], loaded: false, error: null as string | null };
  private listeners = new Set<() => void>();
  private stop: (() => void) | null = null;
  private sequence = 0;
  constructor(private readonly api: FileEditorsApi) {}
  getSnapshot = () => this.snapshot;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private notify() {
    for (const listener of this.listeners) listener();
  }
  accept(items: FileEditorView[]) {
    this.sequence++;
    this.snapshot = { items, loaded: true, error: null };
    this.notify();
  }
  refresh = async () => {
    const sequence = ++this.sequence;
    try {
      const result = await this.api.request({ action: 'list' });
      if (!this.stop || sequence !== this.sequence) return;
      if (!result.ok) throw Error(result.error ?? 'runtime_unavailable');
      this.accept(result.sessions);
    } catch {
      if (!this.stop || sequence !== this.sequence) return;
      this.snapshot = { ...this.snapshot, error: 'runtime_unavailable' };
      this.notify();
    }
  };
  start() {
    if (this.stop) return;
    this.stop = this.api.onChanged(this.refresh);
    void this.refresh();
  }
  dispose() {
    this.stop?.();
    this.stop = null;
    this.sequence++;
  }
}
