import { terminalIsActive } from '../../../shared/workspace-api';
import type { WorkspaceStore } from './workspace-store';

export type ConnectionOpenChoice = 'reuse' | 'new' | 'cancel';
export type NewTerminalBlock = 'connection_limit' | 'total_limit' | 'busy' | null;
export interface ConnectionOpenView {
  connectionId: string;
  name: string;
  count: number;
  newBlocked: NewTerminalBlock;
}
type Request = {
  connectionId: string;
  runtimeId: string | null;
  promise: Promise<string | null>;
  resolve(value: string | null | PromiseLike<string | null>): void;
  detach(): void;
};
export class ConnectionOpenController {
  private request: Request | null = null;
  private view: ConnectionOpenView | null = null;
  private listeners = new Set<() => void>();
  constructor(
    private store: WorkspaceStore,
    private ended: () => void,
    private behavior: () => string = () => 'ask',
  ) {}
  getSnapshot = () => this.view;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(view: ConnectionOpenView | null) {
    if (JSON.stringify(view) === JSON.stringify(this.view)) return;
    this.view = view;
    this.listeners.forEach((listener) => listener());
  }
  private finish(result: string | null | Promise<string | null>) {
    const request = this.request;
    this.request = null;
    request?.detach();
    this.publish(null);
    request?.resolve(result);
  }
  cancel = () => this.finish(null);
  connect() {
    const unsubscribe = this.store.subscribe(this.refresh);
    const unsubscribeFeedback = this.store.feedback.subscribe(this.refresh);
    return () => {
      unsubscribe();
      unsubscribeFeedback();
      this.cancel();
    };
  }
  private blocked(id: string): NewTerminalBlock {
    const active = this.store.getSnapshot().sessions.filter(terminalIsActive);
    if (active.filter((session) => session.connectionId === id).length >= 4)
      return 'connection_limit';
    if (active.length >= 32) return 'total_limit';
    if (this.store.feedback.isBusy('connection:' + id)) return 'busy';
    return null;
  }
  refresh = () => {
    const request = this.request;
    if (!request) return;
    const state = this.store.getSnapshot();
    const connection = state.connections.find((c) => c.id === request.connectionId);
    if (
      !state.ready ||
      state.runtimeId !== request.runtimeId ||
      !connection ||
      connection.deleting
    ) {
      this.cancel();
      return;
    }
    const count = state.sessions.filter(
      (s) => s.connectionId === connection.id && s.phase === 'running',
    ).length;
    if (!count) {
      if (this.view?.count !== 0) this.ended();
    }
    this.publish({
      connectionId: connection.id,
      name: connection.name,
      count,
      newBlocked: this.blocked(connection.id),
    });
  };
  openConnection = (id: string, signal?: AbortSignal): Promise<string | null> => {
    if (signal?.aborted) return Promise.resolve(null);
    if (this.request?.connectionId === id) return this.request.promise;
    const state = this.store.getSnapshot();
    const connection = state.connections.find((c) => c.id === id);
    if (!state.ready || !connection || connection.deleting) return Promise.resolve(null);
    this.cancel();
    if (
      this.store.existingTerminal(id, 'establishing') ||
      this.store.isOpening(id) ||
      !this.store.existingTerminal(id, 'running')
    )
      return this.store.connect(id);
    if (this.behavior() === 'reuse') return this.store.connect(id);
    if (this.behavior() === 'new' && !this.blocked(id)) return this.store.connect(id, true);
    let resolve!: Request['resolve'];
    const promise = new Promise<string | null>((done) => {
      resolve = done;
    });
    const abort = () => {
      if (this.request?.promise === promise) this.cancel();
    };
    this.request = {
      connectionId: id,
      runtimeId: state.runtimeId,
      promise,
      resolve,
      detach: () => signal?.removeEventListener('abort', abort),
    };
    signal?.addEventListener('abort', abort, { once: true });
    this.refresh();
    return promise;
  };
  choose = (choice: ConnectionOpenChoice) => {
    if (choice === 'cancel') {
      this.cancel();
      return;
    }
    this.refresh();
    const request = this.request;
    if (!request) return;
    const id = request.connectionId;
    if (choice === 'reuse') {
      const target =
        this.store.existingTerminal(id, 'establishing') ??
        this.store.existingTerminal(id, 'running');
      if (target) {
        this.finish(target.id);
        this.store.tab(target.id);
      }
    } else if (!this.blocked(id)) {
      // Close the choice before starting an operation whose synchronous events update the store.
      this.request = null;
      request.detach();
      this.publish(null);
      request.resolve(this.store.connect(id, true));
    }
  };
}
