import { EventEmitter } from 'node:events';
import { isDeepStrictEqual } from 'node:util';
import { randomUUID, createHash } from 'node:crypto';
import { createReadStream, watch, type FSWatcher } from 'node:fs';
import { mkdir, readFile, writeFile, rename, copyFile, rm, lstat } from 'node:fs/promises';
import { join, basename } from 'node:path';
import type {
  FileEditCommand,
  FileEditRemote,
  FileEditorView,
  RemoteFileVersion,
  WorkspaceReply,
} from '@contracts';

export interface EditorPorts {
  runtime(): string | null;
  session(sessionId: string): Promise<{ connectionId: string; groupId: string }>;
  application(): string;
  chooseApplication(save: boolean): Promise<string | null>;
  launch(application: string, file: string): Promise<void>;
  begin(command: Extract<FileEditCommand, { action: 'open' }>): Promise<FileEditRemote>;
  call(command: FileEditCommand): {
    id: string;
    runtimeId: string;
    result: Promise<FileEditRemote>;
  };
  reconcile(requestId: string, runtimeId: string): Promise<WorkspaceReply>;
  chooseExport(name: string): Promise<string | null>;
}
type Record = {
  id: string;
  connectionId: string;
  groupId: string;
  path: string;
  application: string;
  filename: string;
  baseline: RemoteFileVersion;
  localHash: string;
  phase: string;
  error: string | null;
  runtimeId: string | null;
  pending?: { requestId: string; runtimeId: string; hash: string; snapshot: string };
};
type Live = {
  record: Record;
  watcher?: FSWatcher;
  timer?: ReturnType<typeof setTimeout>;
  busy: boolean;
  fingerprint?: string;
  writing?: Promise<void>;
  ended?: boolean;
  attempt?: number;
  notificationSignature?: string;
  lastRuntime?: string;
};
export async function hashFile(file: string) {
  const stat = await lstat(file);
  if (!stat.isFile() || stat.isSymbolicLink()) throw Error('files_unsupported');
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(file)) hash.update(bytes);
  return hash.digest('hex');
}
function errorCode(error: unknown) {
  return error instanceof Error ? error.message : 'files_unsupported';
}
export interface FileEditingNotice {
  id: string;
  connectionId: string;
  runtimeId: string;
  attempt: number;
  phase: string;
  unsynced: boolean;
  error: string | null;
}
export class FileEditingService extends EventEmitter<{
  changed: [];
  show: [];
  notification: [FileEditingNotice];
}> {
  private sessions = new Map<string, Live>();
  private opening = new Set<string>();
  private stopped = false;
  private showPending = false;
  private poll: ReturnType<typeof setInterval>;
  readonly owner = randomUUID();
  constructor(
    private root: string,
    private ports: EditorPorts,
  ) {
    super();
    this.poll = setInterval(() => {
      for (const live of this.sessions.values()) void this.observe(live);
    }, 1000);
    this.poll.unref();
  }
  private folder(id: string) {
    return join(this.root, id);
  }
  private file(live: Live) {
    return join(this.folder(live.record.id), live.record.filename);
  }
  list(): FileEditorView[] {
    return [...this.sessions.values()].map(({ record: r }) => ({
      id: r.id,
      connectionId: r.connectionId,
      path: r.path,
      application: basename(r.application, '.app'),
      phase: r.phase,
      error: r.error,
    }));
  }
  get activeCount() {
    return this.sessions.size;
  }
  show() {
    this.showPending = true;
    this.emit('show');
  }
  consumeShow() {
    if (this.showPending) {
      this.showPending = false;
      this.emit('show');
    }
  }
  private async persist(live: Live) {
    if (live.ended) return;
    const content = JSON.stringify(live.record);
    const pending = (live.writing ?? Promise.resolve())
      .catch(() => undefined)
      .then(async () => {
        const directory = this.folder(live.record.id);
        await mkdir(directory, { recursive: true, mode: 0o700 });
        const tmp = join(directory, 'record.pending');
        await writeFile(tmp, content, { mode: 0o600 });
        await rename(tmp, join(directory, 'record.json'));
        this.emit('changed');
      });
    live.writing = pending;
    await pending;
    if (live.ended || this.stopped) return;
    live.lastRuntime = live.record.runtimeId ?? live.lastRuntime;
    const signature = `${live.attempt ?? 0}:${live.record.phase}:${live.record.error}`;
    if (live.notificationSignature === signature) return;
    live.notificationSignature = signature;
    const unsynced =
      !!live.record.pending ||
      (await hashFile(this.file(live)).then(
        (hash) => hash !== live.record.localHash,
        () => true,
      ));
    if (this.stopped || live.ended || live.notificationSignature !== signature) return;
    this.emit('notification', {
      id: live.record.id,
      connectionId: live.record.connectionId,
      runtimeId: live.lastRuntime ?? '',
      attempt: live.attempt ?? 0,
      phase: live.record.phase,
      unsynced,
      error: live.record.error,
    });
  }
  async restore() {
    await mkdir(this.root, { recursive: true, mode: 0o700 });
    const { readdir } = await import('node:fs/promises');
    for (const id of (await readdir(this.root)).slice(0, 32)) {
      if (!/^[0-9a-f-]{36}$/.test(id)) continue;
      try {
        const folder = await lstat(this.folder(id));
        if (!folder.isDirectory() || folder.isSymbolicLink()) continue;
        const metadata = await lstat(join(this.folder(id), 'record.json'));
        if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 65536) continue;
        const r = JSON.parse(
          await readFile(join(this.folder(id), 'record.json'), 'utf8'),
        ) as Record;
        if (
          r.id !== id ||
          typeof r.connectionId !== 'string' ||
          typeof r.path !== 'string' ||
          typeof r.application !== 'string' ||
          typeof r.filename !== 'string' ||
          basename(r.filename) !== r.filename ||
          !r.filename.startsWith('working') ||
          !r.baseline ||
          !/^[a-f0-9]{64}$/.test(r.baseline.hash) ||
          (r.pending &&
            (typeof r.pending.snapshot !== 'string' ||
              basename(r.pending.snapshot) !== r.pending.snapshot ||
              !r.pending.snapshot.startsWith('upload-') ||
              !/^[a-f0-9]{64}$/.test(r.pending.hash))) ||
          !/^[a-f0-9]{64}$/.test(r.localHash)
        )
          continue;
        await lstat(join(this.folder(id), r.filename));
        r.phase = 'disconnected';
        r.error = null;
        r.runtimeId = null;
        this.sessions.set(id, { record: r, busy: false });
      } catch {
        /* Unreadable recovery material is retained rather than deleted. */
      }
    }
    this.emit('changed');
  }
  private require(id: string) {
    const v = this.sessions.get(id);
    if (!v) throw Error('not_found');
    return v;
  }
  private watch(live: Live) {
    live.watcher?.close();
    live.watcher = watch(this.folder(live.record.id), (_event, name) => {
      if (name?.toString() === live.record.filename) void this.observe(live);
    });
    live.watcher.on('error', () => {
      live.watcher?.close();
      live.watcher = undefined;
    });
    void this.observe(live);
  }
  private async observe(live: Live) {
    if (
      this.stopped ||
      live.ended ||
      live.busy ||
      !['synced', 'pending'].includes(live.record.phase)
    )
      return;
    try {
      const s = await lstat(this.file(live));
      if (!s.isFile() || s.isSymbolicLink()) throw Error('files_unsupported');
      if (live.ended) return;
      const fingerprint = `${s.ino}:${s.size}:${s.mtimeMs}:${s.ctimeMs}`;
      if (live.fingerprint === fingerprint) return;
      live.fingerprint = fingerprint;
      if (live.timer) clearTimeout(live.timer);
      live.timer = setTimeout(() => {
        live.timer = undefined;
        void this.synchronize(live).catch(() => undefined);
      }, 750);
    } catch (error) {
      live.record.phase = 'failed';
      live.record.error = errorCode(error);
      await this.persist(live);
    }
  }
  async open(sessionId: string, path: string, alternative: boolean) {
    const session = await this.ports.session(sessionId);
    const key = `${session.groupId}:${path}`;
    if (this.opening.has(key)) throw Error('busy');
    this.opening.add(key);
    try {
      const existing = [...this.sessions.values()].find(
        (v) =>
          v.record.path === path &&
          v.record.runtimeId === this.ports.runtime() &&
          v.record.groupId === session.groupId,
      );
      if (existing) {
        const application =
          alternative || !this.ports.application()
            ? await this.ports.chooseApplication(!alternative)
            : this.ports.application();
        if (!application) return;
        existing.record.application = application;
        await this.persist(existing);
        await this.ports.launch(application, this.file(existing));
        this.show();
        return;
      }
      const application =
        alternative || !this.ports.application()
          ? await this.ports.chooseApplication(!alternative)
          : this.ports.application();
      if (!application) return;
      if (this.sessions.size >= 32) throw Error('busy');
      const id = randomUUID();
      const suffix = basename(path).match(/\.[a-zA-Z0-9_-]{1,16}$/)?.[0] ?? '';
      const filename = `working${suffix}`;
      await mkdir(this.folder(id), { recursive: true, mode: 0o700 });
      const remote = await this.ports.begin({
        action: 'open',
        id,
        leaseOwner: this.owner,
        sessionId,
        path,
        local: join(this.folder(id), filename),
      });
      const duplicate = [...this.sessions.values()].find(
        (v) =>
          v.record.groupId === remote.groupId &&
          v.record.path === remote.path &&
          v.record.runtimeId === this.ports.runtime(),
      );
      if (duplicate) {
        await this.ports.call({ action: 'close', id }).result;
        await rm(this.folder(id), { recursive: true, force: true });
        await this.ports.launch(duplicate.record.application, this.file(duplicate));
        this.show();
        return;
      }
      const live: Live = {
        busy: false,
        record: {
          id,
          connectionId: remote.connectionId,
          groupId: remote.groupId,
          path: remote.path,
          application,
          filename,
          baseline: remote.version,
          localHash: await hashFile(join(this.folder(id), filename)),
          phase: 'synced',
          error: null,
          runtimeId: this.ports.runtime(),
        },
      };
      this.sessions.set(id, live);
      await this.persist(live);
      this.watch(live);
      try {
        await this.ports.launch(application, this.file(live));
      } catch (error) {
        live.record.phase = 'failed';
        live.record.error = errorCode(error);
        await this.persist(live);
        throw error;
      }
      this.show();
    } finally {
      this.opening.delete(key);
    }
  }
  private async synchronize(
    live: Live,
    expected = live.record.baseline,
    destination: string | null = null,
    force = false,
  ) {
    if (live.busy || this.stopped) throw Error('busy');
    if (live.record.runtimeId !== this.ports.runtime()) throw Error('runtime_unavailable');
    if (live.record.pending) throw Error('result_unknown');
    live.busy = true;
    let snapshot = '';
    try {
      const first = await hashFile(this.file(live));
      if (first === live.record.localHash && !destination && !force) {
        live.record.phase = 'synced';
        live.record.error = null;
        await this.persist(live);
        return;
      }
      snapshot = join(this.folder(live.record.id), `upload-${randomUUID()}`);
      await copyFile(this.file(live), snapshot);
      const hash = await hashFile(snapshot);
      if (hash !== first || (await hashFile(this.file(live))) !== hash) {
        await rm(snapshot, { force: true });
        live.fingerprint = undefined;
        live.record.phase = 'pending';
        return;
      }
      live.attempt = (live.attempt ?? 0) + 1;
      live.record.phase = 'uploading';
      live.record.error = null;
      const request = this.ports.call({
        action: 'upload',
        id: live.record.id,
        local: snapshot,
        expected,
        destination,
      });
      live.record.pending = {
        requestId: request.id,
        runtimeId: request.runtimeId,
        hash,
        snapshot: basename(snapshot),
      };
      await this.persist(live);
      const remote = await request.result;
      live.record.baseline = remote.version;
      live.record.path = remote.path;
      live.record.localHash = hash;
      delete live.record.pending;
      live.record.phase =
        this.stopped || live.record.runtimeId !== this.ports.runtime() ? 'disconnected' : 'synced';
      await this.persist(live);
      await rm(snapshot, { force: true });
    } catch (error) {
      const code = errorCode(error);
      if (['timeout', 'runtime_unavailable', 'result_unknown'].includes(code))
        live.record.phase =
          this.stopped || live.record.runtimeId !== this.ports.runtime()
            ? 'disconnected'
            : 'unknown';
      else {
        live.record.phase =
          code === 'conflict'
            ? 'conflict'
            : ['disconnected', 'tools_unavailable'].includes(code)
              ? 'disconnected'
              : 'failed';
        delete live.record.pending;
        if (snapshot) await rm(snapshot, { force: true });
      }
      live.record.error = code;
      await this.persist(live);
      throw error;
    } finally {
      live.busy = false;
      live.fingerprint = undefined;
      if (live.record.phase === 'synced' || live.record.phase === 'pending')
        void this.observe(live);
    }
  }
  async retry(id: string) {
    const live = this.require(id);
    if (live.busy) throw Error('busy');
    if (live.record.pending) {
      const p = live.record.pending;
      const result = await this.ports.reconcile(p.requestId, p.runtimeId);
      if (result.kind !== 'reconciled') throw Error('invalid_response');
      if (!result.result) {
        if (result.error) {
          delete live.record.pending;
          live.record.phase = result.error === 'conflict' ? 'conflict' : 'failed';
          live.record.error = result.error;
          await this.persist(live);
          await rm(join(this.folder(id), p.snapshot), { force: true });
          throw Error(result.error);
        }
        throw Error('result_unknown');
      }
      if (result.result.kind !== 'fileEdit') throw Error('invalid_response');
      live.record.baseline = result.result.edit.version;
      live.record.path = result.result.edit.path;
      live.record.localHash = p.hash;
      await rm(join(this.folder(id), p.snapshot), { force: true });
      delete live.record.pending;
      live.record.phase = 'synced';
      live.record.error = null;
      await this.persist(live);
    }
    await this.synchronize(live);
  }
  async resolve(id: string, choice: string, destination: string | null) {
    const live = this.require(id);
    if (live.busy || live.record.pending) throw Error('busy');
    if (choice === 'overwrite') {
      const current = await this.ports.call({ action: 'inspect', id }).result;
      await this.synchronize(live, current.version, null, true);
    } else if (choice === 'saveAs') {
      if (!destination || !destination.startsWith('/') || destination === live.record.path)
        throw Error('invalid_input');
      await this.synchronize(live, live.record.baseline, destination);
    } else if (choice === 'remote') {
      live.busy = true;
      const incoming = join(this.folder(id), `remote-${randomUUID()}`);
      try {
        const result = await this.ports.call({ action: 'reload', id, local: incoming }).result;
        await rename(incoming, this.file(live));
        live.record.baseline = result.version;
        live.record.localHash = await hashFile(this.file(live));
        live.record.phase = 'synced';
        live.record.error = null;
        await this.persist(live);
      } finally {
        live.busy = false;
        live.fingerprint = undefined;
      }
    } else throw Error('invalid_input');
  }
  async recover(id: string, sessionId: string) {
    const live = this.require(id);
    if (live.busy) throw Error('busy');
    if ((await this.ports.session(sessionId)).connectionId !== live.record.connectionId)
      throw Error('connection_changed');
    live.busy = true;
    live.attempt = (live.attempt ?? 0) + 1;
    const incoming = join(this.folder(id), `recovery-${randomUUID()}`);
    try {
      if (live.record.runtimeId === this.ports.runtime())
        await this.ports.call({ action: 'close', id }).result;
      const remote = await this.ports.begin({
        action: 'open',
        id,
        leaseOwner: this.owner,
        sessionId,
        path: live.record.path,
        local: incoming,
      });
      if (remote.connectionId !== live.record.connectionId) {
        await this.ports.call({ action: 'close', id }).result;
        throw Error('connection_changed');
      }
      live.record.runtimeId = this.ports.runtime();
      live.record.groupId = remote.groupId;
      const unchanged = isDeepStrictEqual(remote.version, live.record.baseline);
      live.record.phase = unchanged ? 'pending' : 'conflict';
      live.record.error = unchanged ? null : 'conflict';
      // A new runtime cannot reconcile an old receipt. Its content is compared before any upload.
      if (
        live.record.pending &&
        remote.version.hash === live.record.pending.hash &&
        remote.version.permissions === live.record.baseline.permissions &&
        remote.version.uid === live.record.baseline.uid &&
        remote.version.gid === live.record.baseline.gid
      ) {
        live.record.localHash = live.record.pending.hash;
        live.record.baseline = remote.version;
        live.record.phase = 'synced';
        live.record.error = null;
      }
      delete live.record.pending;
      await this.persist(live);
      this.watch(live);
    } finally {
      live.busy = false;
      await rm(incoming, { force: true });
    }
    if (live.record.phase === 'pending' || live.record.phase === 'synced')
      await this.synchronize(live);
  }
  async reopen(id: string) {
    const live = this.require(id);
    try {
      await this.ports.launch(live.record.application, this.file(live));
    } catch {
      const selected = await this.ports.chooseApplication(false);
      if (selected) {
        live.record.application = selected;
        await this.persist(live);
        await this.ports.launch(selected, this.file(live));
      }
    }
  }
  async export(id: string) {
    const live = this.require(id);
    const target = await this.ports.chooseExport(basename(live.record.path));
    if (target) await copyFile(this.file(live), target);
  }
  async end(id: string, discard: boolean) {
    const live = this.require(id);
    if (live.busy || live.record.pending) throw Error('busy');
    if (!discard && (await hashFile(this.file(live))) !== live.record.localHash)
      throw Error('file_editor_unsynced');
    if (live.record.runtimeId === this.ports.runtime())
      await this.ports.call({ action: 'close', id }).result;
    live.watcher?.close();
    if (live.timer) clearTimeout(live.timer);
    live.ended = true;
    this.sessions.delete(id);
    await live.writing;
    await rm(this.folder(id), { recursive: true, force: true });
    this.emit('changed');
  }
  async disconnect() {
    for (const live of this.sessions.values()) {
      live.watcher?.close();
      live.watcher = undefined;
      if (live.timer) clearTimeout(live.timer);
      live.lastRuntime = live.record.runtimeId ?? live.lastRuntime;
      live.record.phase = 'disconnected';
      live.record.runtimeId = null;
      await this.persist(live);
    }
  }
  async stop() {
    this.stopped = true;
    clearInterval(this.poll);
    await this.disconnect();
  }
}
