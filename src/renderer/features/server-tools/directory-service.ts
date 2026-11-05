import { loadUiState, readUiState, updateUiState } from '../../shared/platform/ui-state-store';
import type { RemoteEntry, TerminalSessionView, IdentityName, WorkspaceReply } from '@contracts';
import type { WorkspaceStore } from '../connections/workspace-store';

export type Directory = { path: string; entries: RemoteEntry[] };
type Job = {
  key: string;
  session: TerminalSessionView;
  path: string;
  priority: number;
  request: string;
  users: number;
  started: boolean;
  cancelled: boolean;
  resolve(value: Directory): void;
  reject(error: unknown): void;
  promise: Promise<Directory>;
};
const cancelled = () => new DOMException('Cancelled', 'AbortError');
export class DirectoryService {
  private jobs = new Map<string, Job>();
  private cache = new Map<string, Directory>();
  private names = new Map<string, { time: number; value: IdentityName }>();
  private pinned = new Set<string>();
  private active = 0;
  private runtime: string | null = null;
  readonly preferences = new Map<
    string,
    {
      path: string;
      history: string[];
      index: number;
      scroll: number;
      expanded: Set<string>;
    }
  >();
  constructor(private store: WorkspaceStore) {}
  preference(id: string) {
    let value = this.preferences.get(id);
    if (!value) {
      value = {
        path: '.',
        history: [],
        index: -1,
        scroll: 0,
        expanded: new Set(['/']),
      };
      this.preferences.set(id, value);
    }
    return value;
  }
  private restored = new Map<string, { revision: number; groupId: string }>();
  private restoreRequests = new Map<string, { revision: number; groupId: string }>();
  async restore(session: TerminalSessionView, mode = 'initial') {
    const restored = this.restored.get(session.connectionId);
    if (
      restored &&
      restored.revision === session.connectionRevision &&
      restored.groupId === session.groupId
    )
      return;
    const request = {
      revision: session.connectionRevision,
      groupId: session.groupId,
    };
    this.restoreRequests.set(session.connectionId, request);
    if (mode === 'last') await loadUiState(`connection:${session.connectionId}`);
    if (this.restoreRequests.get(session.connectionId) !== request) return;
    const saved = mode === 'last' ? readUiState(`connection:${session.connectionId}`).data : null;
    const pref = this.preference(session.connectionId);
    pref.path = '.';
    pref.scroll = 0;
    pref.history = [];
    pref.index = -1;
    pref.expanded = new Set(['/']);
    if (saved?.kind === 'directory' && saved.connectionRevision === session.connectionRevision) {
      pref.path = saved.path;
      pref.scroll = saved.scroll;
      pref.expanded = new Set(saved.expanded);
    }
    this.restored.set(session.connectionId, {
      revision: session.connectionRevision,
      groupId: session.groupId,
    });
  }
  persist(session: TerminalSessionView) {
    const pref = this.preference(session.connectionId);
    const expanded: string[] = [];
    let bytes = 0;
    for (const path of pref.expanded) {
      bytes += new TextEncoder().encode(path).length;
      if (expanded.length >= 64 || bytes > 24000) break;
      expanded.push(path);
    }
    void updateUiState(`connection:${session.connectionId}`, {
      kind: 'directory',
      connectionRevision: session.connectionRevision,
      path: pref.path,
      scroll: Math.max(0, Math.round(pref.scroll)),
      expanded,
    });
  }
  private key(session: TerminalSessionView, path: string) {
    return `${session.groupId}\0${path}`;
  }
  prune(groups: Set<string>) {
    for (const [id, value] of this.restoreRequests)
      if (!groups.has(value.groupId)) this.restoreRequests.delete(id);
    for (const [id, value] of this.restored)
      if (!groups.has(value.groupId)) this.restored.delete(id);
    for (const key of this.cache.keys())
      if (!groups.has(key.split('\0')[0]!)) this.cache.delete(key);
    for (const key of this.names.keys())
      if (!groups.has(key.split(':')[0]!)) this.names.delete(key);
    for (const job of this.jobs.values())
      if (!groups.has(job.session.groupId)) this.invalidate(job.session.groupId);
  }
  pin(group: string, path: string) {
    const key = `${group}\0${path}`;
    this.pinned.add(key);
    return () => {
      this.pinned.delete(key);
    };
  }
  invalidate(group: string, path?: string) {
    for (const key of this.cache.keys())
      if (key.startsWith(`${group}\0`) && (!path || key === `${group}\0${path}`))
        this.cache.delete(key);
    for (const [key, job] of this.jobs)
      if (key.startsWith(`${group}\0`) && (!path || key === `${group}\0${path}`)) {
        job.cancelled = true;
        job.reject(cancelled());
        this.jobs.delete(key);
        if (job.started)
          void this.store.call('cancelFileList', { requestId: job.request }).catch(() => {});
      }
  }
  read(
    session: TerminalSessionView,
    path: string,
    signal: AbortSignal,
    priority = 1,
  ): Promise<Directory> {
    if (signal.aborted) return Promise.reject(cancelled());
    const runtime = this.store.getSnapshot().runtimeId;
    if (runtime !== this.runtime) {
      for (const job of this.jobs.values()) {
        job.cancelled = true;
        job.reject(cancelled());
        if (job.started)
          void this.store.call('cancelFileList', { requestId: job.request }).catch(() => {});
      }
      this.jobs.clear();
      this.cache.clear();
      this.names.clear();
      this.runtime = runtime;
    }
    const key = this.key(session, path);
    const cached = this.cache.get(key);
    if (cached) {
      this.cache.delete(key);
      this.cache.set(key, cached);
      return Promise.resolve(cached);
    }
    let job = this.jobs.get(key);
    if (!job) {
      let resolve!: Job['resolve'], reject!: Job['reject'];
      const promise = new Promise<Directory>((yes, no) => {
        resolve = yes;
        reject = no;
      });
      job = {
        key,
        session,
        path,
        priority,
        request: crypto.randomUUID(),
        users: 0,
        started: false,
        cancelled: false,
        resolve,
        reject,
        promise,
      };
      this.jobs.set(key, job);
    }
    const target = job;
    target.users++;
    target.priority = Math.min(target.priority, priority);
    const result = new Promise<Directory>((resolve, reject) => {
      let settled = false;
      const release = () => {
        if (settled) return false;
        settled = true;
        signal.removeEventListener('abort', abort);
        target.users--;
        if (!target.users && this.jobs.get(key) === target) {
          target.cancelled = true;
          this.jobs.delete(key);
          if (target.started)
            void this.store.call('cancelFileList', { requestId: target.request }).catch(() => {});
        }
        return true;
      };
      const abort = () => {
        if (release()) reject(cancelled());
      };
      signal.addEventListener('abort', abort, { once: true });
      target.promise.then(
        (value) => {
          if (release()) resolve(value);
        },
        (error) => {
          if (release()) reject(error);
        },
      );
    });
    this.pump();
    return result;
  }
  private pump() {
    const pending = [...this.jobs.values()]
      .filter((j) => !j.started && !j.cancelled)
      .sort((a, b) => a.priority - b.priority);
    while (this.active < 4 && pending.length) {
      const job = pending.shift()!;
      job.started = true;
      this.active++;
      void this.run(job);
    }
  }
  private async run(job: Job) {
    try {
      let cursor: string | null = null,
        path = job.path;
      const entries: RemoteEntry[] = [];
      do {
        if (job.cancelled) throw cancelled();
        const reply: WorkspaceReply = await this.store.call('listFiles', {
          sessionId: job.session.id,
          path: job.path,
          requestId: job.request,
          cursor,
        });
        if (reply.kind !== 'directory') throw Error('protocol_mismatch');
        path = reply.page.path;
        entries.push(...reply.page.entries);
        if (
          entries.length > 20000 ||
          (reply.page.next !== null && Number(reply.page.next) <= Number(cursor ?? -1))
        )
          throw Error('protocol_mismatch');
        cursor = reply.page.next;
      } while (cursor !== null);
      if (job.cancelled) throw cancelled();
      const value = { path, entries };
      const key = this.key(job.session, path);
      this.cache.set(key, value);
      let count = [...this.cache.values()].reduce((n, d) => n + d.entries.length, 0);
      for (const [old, d] of this.cache) {
        if (this.cache.size <= 32 && count <= 20000) break;
        if (old !== key && !this.pinned.has(old)) {
          this.cache.delete(old);
          count -= d.entries.length;
        }
      }
      if (count > 20000 || this.cache.size > 32) {
        if (!this.pinned.has(key)) this.cache.delete(key);
      }
      job.resolve(value);
    } catch (error) {
      job.reject(error);
    } finally {
      await this.store.call('releaseFileList', { requestId: job.request }).catch(() => {});
      if (this.jobs.get(job.key) === job) this.jobs.delete(job.key);
      this.active--;
      this.pump();
    }
  }
  async identities(session: TerminalSessionView, entries: RemoteEntry[]) {
    const result = new Map<string, string>(),
      uids = new Set<number>(),
      gids = new Set<number>();
    const now = Date.now();
    for (const entry of entries)
      for (const [group, id] of [
        [false, entry.uid],
        [true, entry.gid],
      ] as const) {
        if (id === null) continue;
        const short = `${group ? 'g' : 'u'}:${id}`,
          key = `${session.groupId}:${short}`;
        const cached = this.names.get(key);
        if (cached && now - cached.time < 300000) result.set(short, cached.value.name);
        else (group ? gids : uids).add(id);
      }
    const requests = [...uids]
      .map((id) => ({ id, group: false }))
      .concat([...gids].map((id) => ({ id, group: true })));
    for (let offset = 0; offset < requests.length; offset += 64) {
      if (
        !this.store.getSnapshot().sessions.some((s) => s.id === session.id && s.phase === 'running')
      )
        break;
      const batch = requests.slice(offset, offset + 64);
      const reply = await this.store
        .call('resolveFileIdentities', {
          sessionId: session.id,
          uids: batch.filter((x) => !x.group).map((x) => x.id),
          gids: batch.filter((x) => x.group).map((x) => x.id),
        })
        .catch(() => null);
      if (reply?.kind !== 'identities') continue;
      for (const name of reply.names) {
        const key = `${name.group ? 'g' : 'u'}:${name.id}`;
        result.set(key, name.name);
        this.names.set(`${session.groupId}:${key}`, { time: now, value: name });
      }
      while (this.names.size > 1024) this.names.delete(this.names.keys().next().value!);
    }
    return result;
  }
}
const services = new WeakMap<WorkspaceStore, DirectoryService>();
export function directories(store: WorkspaceStore) {
  let service = services.get(store);
  if (!service) {
    service = new DirectoryService(store);
    services.set(store, service);
  }
  return service;
}
