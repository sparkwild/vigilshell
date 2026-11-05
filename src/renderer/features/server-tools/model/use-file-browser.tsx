import { renamedPath } from '../file-menu-target';
import { IconButton } from '../../../components/ui/icon-button';

import type { PanelImperativeHandle } from 'react-resizable-panels';

import { usePreferences } from '../../settings/use-preferences';

import { TruncatedText } from '../../../components/ui/tooltip';

import { useEffect, useEffectEvent, useCallback, useMemo, useRef, useState, useId } from 'react';

import type { AgentTask, RemoteEntry, TerminalSessionView } from '@contracts';
import { agentFileLocation } from './use-agent-file-tasks';

import { type Selection, type SortDescriptor } from '../../../components/ui/data-table';

import { FolderIcon, FileIcon, LinkIcon } from 'lucide-react';

import { useDesktop } from '../../../app/DesktopProvider';

import { useWorkspace, errorCode } from '../../connections/workspace-context';

import { useWorkspaceUi } from '../../connections/ui-context';

import { useConfirmation } from '../../../shared/ConfirmationProvider';

import { directories, type Directory } from '../directory-service';

import { fileSize, validFileName, permissions, sortFiles, parentPath } from '../file-format';

export function useFileBrowser({
  session,
  online,
  visible,
  agentTasks,
}: {
  session: TerminalSessionView;
  online: boolean;
  visible: boolean;
  panelActions?: React.ReactNode;
  agentTasks?: AgentTask[];
}) {
  const { store, state } = useWorkspace(),
    { t } = useDesktop(),
    confirm = useConfirmation(),
    service = directories(store);
  const { setToolDialogClose } = useWorkspaceUi();
  const pref = service.preference(session.connectionId);
  const preferences = usePreferences();
  const [layout] = useState(() => ({
    widths: new Map<string, number>(),
    treeWidth: 180,
  }));
  const treePanel = useRef<PanelImperativeHandle | null>(null);
  const [directory, setDirectory] = useState<Directory>({
    path: pref.path,
    entries: [],
  });
  const [typed, setTyped] = useState(pref.path),
    [editingPath, setEditingPath] = useState(false);
  const [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection>(new Set()),
    [sort, setSort] = useState<SortDescriptor>({
      column: 'name',
      direction: 'ascending',
    });
  const [filter, setFilter] = useState('');
  const hidden = preferences.preferences.filesShowHidden,
    columns = preferences.preferences.filesColumns;
  const [narrowTree, setNarrowTree] = useState(false);
  const tree = preferences.preferences.filesShowTree;
  const [width, setWidth] = useState(800),
    [treeRevision, setTreeRevision] = useState(0);
  const [names, setNames] = useState(new Map<string, string>());
  const [edit, setEdit] = useState<{
      entry: RemoteEntry | null;
      name: string;
      directory?: string;
      kind?: 'file' | 'directory';
    } | null>(null),
    [property, setProperty] = useState<RemoteEntry | null>(null);
  const [permissionEntries, setPermissionEntries] = useState<RemoteEntry[] | null>(null);
  const [permissionError, setPermissionError] = useState<string | null>(null);
  const [operationsOpen, setOperationsOpen] = useState(false),
    [operationId, setOperationId] = useState<string | null>(null);
  const context = useRef({
    runtimeId: state.runtimeId,
    groupId: session.groupId,
    sessionId: session.id,
  });
  context.current = { runtimeId: state.runtimeId, groupId: session.groupId, sessionId: session.id };
  const current = () => ({ ...context.current });
  const valid = (captured: ReturnType<typeof current>) =>
    mounted.current &&
    captured.runtimeId === context.current.runtimeId &&
    captured.groupId === context.current.groupId &&
    captured.sessionId === context.current.sessionId;

  const root = useRef<HTMLDivElement>(null),
    scroll = useRef<HTMLDivElement>(null),
    request = useRef<AbortController | null>(null),
    mounted = useRef(true);
  const lastReadFailure = useRef<string | null>(null);
  const editNameId = useId();
  const [editError, setEditError] = useState<string | null>(null);
  const [editBusy, setEditBusy] = useState(false);
  const editLock = useRef(false);
  const editClosing = useRef(false);
  const closeNameDialog = useCallback(async () => {
    if (!edit || editLock.current || editClosing.current) return;
    const current = edit;
    editClosing.current = true;
    try {
      if (
        current.name !== (current.entry?.name ?? '') &&
        !(await confirm({
          tone: 'warning',
          title: t('ws_discard_title'),
          description: t('ui_discard_form_body'),
          confirmLabel: t('ws_confirm'),
          cancelLabel: t('ws_cancel'),
        }))
      )
        return;
      setEdit((value) => (value === current ? null : value));
      setEditError(null);
    } finally {
      editClosing.current = false;
    }
  }, [edit, confirm, t]);
  async function submitName() {
    if (!edit || disabled || editLock.current) return;
    if (!validFileName(edit.name)) {
      setEditError('invalid_input');
      return;
    }
    const current = edit;
    editLock.current = true;
    setEditBusy(true);
    setEditError(null);
    try {
      const saved = await perform(
        () =>
          current.entry
            ? store.mutation('renameFile', {
                sessionId: session.id,
                from: current.entry.path,
                to: renamedPath(current.entry, current.name),
              })
            : store.mutation(current.kind === 'file' ? 'createFile' : 'createDirectory', {
                sessionId: session.id,
                path: `${(current.directory ?? directory.path).replace(/\/$/, '')}/${current.name}`,
              }),
        setEditError,
        current.entry?.kind === 'directory' &&
          (directory.path === current.entry.path ||
            directory.path.startsWith(current.entry.path + '/'))
          ? renamedPath(current.entry, current.name) +
              directory.path.slice(current.entry.path.length)
          : directory.path,
        () => {
          if (current.entry?.kind !== 'directory') return;
          const old = current.entry.path,
            next = renamedPath(current.entry, current.name);
          const move = (path: string) =>
            path === old || path.startsWith(old + '/') ? next + path.slice(old.length) : path;
          pref.history = pref.history.map(move);
          pref.expanded = new Set([...pref.expanded].map(move));
          service.persist(session);
        },
      );
      if (saved && mounted.current) setEdit((value) => (value === current ? null : value));
    } finally {
      editLock.current = false;
      if (mounted.current) setEditBusy(false);
    }
  }
  const dialogOpen = !!edit || !!property;
  useEffect(() => {
    if (!dialogOpen) return;
    setToolDialogClose(() => {
      if (edit) void closeNameDialog();
      else setProperty(null);
    });
    return () => setToolDialogClose(null);
  }, [dialogOpen, edit, setToolDialogClose, closeNameDialog]);
  useEffect(
    () => service.pin(session.groupId, directory.path),
    [service, session.groupId, directory.path],
  );
  const disabled = !online || loading || busy || !!session.agentControl;
  const followed = useRef('');
  const restored = useRef(false);
  const followAgentFiles = useEffectEvent((tasks: AgentTask[]) => {
    const task = tasks
      .filter((task) => task.contextId === session.agentControl?.contextId)
      .sort((a, b) => b.createdAt - a.createdAt || b.id.localeCompare(a.id))[0];
    if (!task) {
      followed.current = '';
      return;
    }
    const key = `${session.agentControl?.leaseId}:${task.id}:${task.phase}:${task.fileEffects}`;
    if (followed.current === key) return;
    followed.current = key;
    const location = agentFileLocation(task);
    if (!location) return;
    service.invalidate(session.groupId);
    setTreeRevision((value) => value + 1);
    setFilter('');
    void load(location.directory, undefined, true, false, location.paths);
  });
  useEffect(() => {
    if (!session.agentControl?.leaseId && followed.current) {
      request.current?.abort();
      followed.current = '';
      setLoading(false);
    }
    if (online && visible && restored.current) followAgentFiles(agentTasks ?? []);
  }, [agentTasks, online, visible, session.agentControl?.leaseId]);

  const pollAgentDirectory = useEffectEvent(async (signal: AbortSignal) => {
    if (loading || busy || !directory.path) return;
    const captured = current(),
      path = directory.path,
      foreground = request.current;
    service.invalidate(session.groupId, path);
    try {
      const result = await service.read(session, path, signal, 0);
      if (
        signal.aborted ||
        !valid(captured) ||
        pref.path !== path ||
        foreground !== request.current
      )
        return;
      setDirectory(result);
    } catch (cause) {
      if (!signal.aborted && valid(captured)) setError(errorCode(cause));
    }
  });
  useEffect(() => {
    if (!session.agentControl?.leaseId || !visible || !online) return;
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      await pollAgentDirectory(abort.signal);
      if (!abort.signal.aborted) timer = setTimeout(() => void poll(), 2500);
    };
    timer = setTimeout(() => void poll(), 2500);
    return () => {
      abort.abort();
      clearTimeout(timer);
    };
  }, [session.agentControl?.leaseId, visible, online]);
  const files = useMemo(
    () =>
      sortFiles(
        directory.entries.filter(
          (e) =>
            (hidden || !e.name.startsWith('.')) &&
            e.name.toLocaleLowerCase().includes(filter.toLocaleLowerCase()),
        ),
        String(sort.column),
        sort.direction === 'descending',
      ),
    [directory, hidden, filter, sort],
  );
  const chosen = selection === 'all' ? files : files.filter((e) => selection.has(e.path));
  async function load(
    path: string,
    historyIndex?: number,
    refresh = false,
    restoring = false,
    focusPaths: string[] = [],
  ) {
    if (!online) return;
    request.current?.abort();
    const abort = new AbortController();
    request.current = abort;
    if (refresh) service.invalidate(session.groupId, path);
    setLoading(true);
    lastReadFailure.current = null;
    setError(null);
    setTyped(path);
    try {
      const result = await service.read(session, path, abort.signal, 0);
      if (abort.signal.aborted || !mounted.current) return;
      setDirectory(result);
      setTyped(result.path);
      setEditingPath(false);
      setSelection(
        new Set(
          result.entries
            .filter((entry) => focusPaths.includes(entry.path))
            .map((entry) => entry.path),
        ),
      );
      setNames(new Map());
      pref.path = result.path;
      if (historyIndex !== undefined) pref.index = historyIndex;
      else if (pref.history[pref.index] !== result.path) {
        pref.history = pref.history.slice(0, pref.index + 1);
        pref.history.push(result.path);
        if (pref.history.length > 100) pref.history.shift();
        pref.index = pref.history.length - 1;
      }
      if (!refresh && !restoring) pref.scroll = 0;
      if (scroll.current)
        requestAnimationFrame(() => {
          if (!abort.signal.aborted && scroll.current) scroll.current.scrollTop = pref.scroll;
        });
      service.persist(session);
      void service.identities(session, result.entries).then((value) => {
        if (!abort.signal.aborted && mounted.current) setNames(value);
      });
      return true;
    } catch (e) {
      if (!abort.signal.aborted && mounted.current) {
        const code = errorCode(e);
        lastReadFailure.current = code;
        if (
          restoring &&
          path !== '.' &&
          [
            'file_not_found',
            'permission_denied',
            'sftp_no_such_file',
            'sftp_permission_denied',
            'files_permission_denied',
            'not_found',
          ].includes(code)
        ) {
          pref.path = '.';
          pref.scroll = 0;
          await load('.', undefined, false, false);
          if (!request.current?.signal.aborted) setError('restored_path_unavailable');
        } else {
          setError(code);
          setTyped(pref.path);
        }
      }
      return false;
    } finally {
      if (!abort.signal.aborted && mounted.current) setLoading(false);
    }
  }
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      request.current?.abort();
    };
  }, []);
  const restoreDirectory = useEffectEvent(async (cancelled: () => boolean) => {
    await service.restore(session, preferences.preferences.remoteDirectoryMode);
    if (!cancelled()) {
      restored.current = true;
      setTreeRevision((n) => n + 1);
      if (session.agentControl && agentTasks?.length) {
        followed.current = '';
        followAgentFiles(agentTasks);
      } else void load(pref.path, undefined, false, true);
    }
  });
  useEffect(() => {
    let cancelled = false;
    restored.current = false;
    if (visible && online) void restoreDirectory(() => cancelled);
    else request.current?.abort();
    return () => {
      cancelled = true;
      request.current?.abort();
    };
  }, [visible, online, session.id]);
  useEffect(() => {
    if (!columns.includes(String(sort.column))) {
      setSort({ column: 'name', direction: 'ascending' });
    }
  }, [columns, sort.column]);
  useEffect(() => {
    if (!root.current) return;
    const observer = new ResizeObserver(([e]) => setWidth(e!.contentRect.width));
    observer.observe(root.current);
    return () => observer.disconnect();
  }, []);
  const completed = [...state.transfers.values()]
    .filter(
      (task) =>
        task.groupId === session.groupId &&
        task.direction === 'upload' &&
        task.phase === 'complete',
    )
    .map((task) => task.id)
    .join(',');
  const previousCompleted = useRef(completed);
  const uploaded = useEffectEvent(() => {
    service.invalidate(session.groupId);
    if (visible && online) void load(pref.path, undefined, true);
  });
  useEffect(() => {
    if (previousCompleted.current === completed) return;
    previousCompleted.current = completed;
    uploaded();
  }, [completed]);
  async function perform(
    action: () => Promise<unknown>,
    onError?: (code: string) => void,
    refreshPath = directory.path,
    afterSuccess?: () => void,
  ) {
    if (disabled) return false;
    const path = refreshPath;
    setBusy(true);
    setError(null);
    try {
      await action();
      afterSuccess?.();
      service.invalidate(session.groupId);
      if (mounted.current) {
        setTreeRevision((v) => v + 1);
        await load(path, undefined, true);
      }
      return true;
    } catch (e) {
      if (mounted.current) {
        setError(errorCode(e));
        onError?.(errorCode(e));
      }
      return false;
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function transfer(
    direction: 'upload' | 'download',
    paths = chosen.map((entry) => entry.path),
    targetDirectory = directory.path,
    archive = false,
  ) {
    if (disabled || (direction === 'download' && (!paths.length || paths.length > 100))) return;
    const captured = current();
    setBusy(true);
    setError(null);
    try {
      const reply = await store.call('pickFiles', { direction });
      if (!valid(captured) || reply.kind !== 'fileGrant') return;
      if (archive)
        await store.mutation('archiveFiles', {
          sessionId: captured.sessionId,
          grantId: reply.grant.id,
          paths,
        });
      else
        await store.mutation('startTransfer', {
          sessionId: captured.sessionId,
          grantId: reply.grant.id,
          paths: direction === 'upload' ? [targetDirectory] : paths,
          overwrite: false,
        });
      if (valid(captured)) await store.refreshTransfers();
    } catch (cause) {
      if (valid(captured)) setError(errorCode(cause));
    } finally {
      if (valid(captured)) setBusy(false);
    }
  }
  async function inspectPaths(paths: string[]) {
    if (disabled || !paths.length || paths.length > 100) return null;
    const captured = current(),
      entries: RemoteEntry[] = [];
    setBusy(true);
    setError(null);
    try {
      for (const path of paths) {
        if (!valid(captured)) return null;
        const reply = await store.call('statFile', { sessionId: captured.sessionId, path });
        if (reply.kind !== 'fileEntry') throw new Error('invalid_response');
        entries.push(reply.entry);
      }
      return valid(captured) ? entries : null;
    } catch (cause) {
      if (valid(captured)) setError(errorCode(cause));
      return null;
    } finally {
      if (valid(captured)) setBusy(false);
    }
  }
  async function showProperties(path: string) {
    const entries = await inspectPaths([path]);
    if (entries?.[0]) setProperty(entries[0]);
  }
  async function showPermissions(paths: string[]) {
    const entries = await inspectPaths(paths);
    if (!entries) return;
    if (
      entries.some(
        (entry) => !['file', 'directory'].includes(entry.kind) || entry.permissions === null,
      )
    ) {
      setError('files_unsupported');
      return;
    }
    setPermissionError(null);
    setPermissionEntries(entries);
  }
  async function startFileOperation(
    paths: string[],
    operation: 'remove' | 'permissions',
    recursive: boolean,
    setMask = 0,
    clearMask = 0,
  ) {
    const captured = current();
    setBusy(true);
    setError(null);
    setPermissionError(null);
    try {
      const reply = await store.mutation('startFileOperation', {
        sessionId: captured.sessionId,
        paths,
        operation,
        recursive,
        setMask,
        clearMask,
      });
      if (!valid(captured)) return false;
      if (reply.kind !== 'operation') throw new Error('invalid_response');
      setOperationId(reply.operation.id);
      setOperationsOpen(true);
      return true;
    } catch (cause) {
      if (valid(captured)) {
        if (operation === 'permissions') setPermissionError(errorCode(cause));
        else setError(errorCode(cause));
      }
      return false;
    } finally {
      if (valid(captured)) setBusy(false);
    }
  }
  async function remove(paths: string[]) {
    if (disabled || !paths.length || paths.length > 100 || paths.includes('/')) return;
    const captured = current();
    const confirmed = await confirm({
      title: t('files_delete_items_title'),
      description: t('files_delete_items_body', { count: paths.length }),
      detail: (
        <div className="max-h-64 overflow-auto">
          {paths.map((path) => (
            <div key={path}>
              <TruncatedText>{path}</TruncatedText>
            </div>
          ))}
        </div>
      ),
      tone: 'danger',
      confirmLabel: t('ws_confirm'),
      cancelLabel: t('ws_cancel'),
    });
    if (confirmed && valid(captured)) await startFileOperation(paths, 'remove', true);
  }
  async function openEntry(entry: RemoteEntry, alternative = false) {
    if (disabled) return;
    if (entry.kind === 'directory' || entry.kind === 'symlink') {
      await load(entry.path);
      return;
    }
    if (entry.kind !== 'file') return;
    const captured = current();
    setBusy(true);
    setError(null);
    try {
      const reply = await window.vigilshell.fileEditors.request({
        action: 'open',
        sessionId: captured.sessionId,
        path: entry.path,
        alternative,
      });
      if (valid(captured) && !reply.ok) setError(reply.error ?? 'tools_unavailable');
    } catch (cause) {
      if (valid(captured)) setError(errorCode(cause));
    } finally {
      if (valid(captured)) setBusy(false);
    }
  }
  const finishedFileOperations = [...state.operations.values()]
    .filter(
      (operation) =>
        operation.kind === 'files' &&
        operation.sessionId === session.id &&
        !['working', 'trust'].includes(operation.phase),
    )
    .map((operation) => operation.id + ':' + operation.phase)
    .join(',');
  const previousFileOperations = useRef(finishedFileOperations);
  const refreshFileOperations = useEffectEvent(async () => {
    const captured = current();
    service.invalidate(session.groupId);
    setTreeRevision((n) => n + 1);
    let path = pref.path;
    while (valid(captured) && online) {
      if (await load(path, undefined, true)) break;
      if (
        !valid(captured) ||
        path === '/' ||
        path === '.' ||
        !['file_not_found', 'sftp_no_such_file', 'not_found'].includes(
          lastReadFailure.current ?? '',
        )
      )
        break;
      const prior = path;
      path = parentPath(path);
      pref.expanded = new Set(
        [...pref.expanded].filter((value) => value !== prior && !value.startsWith(prior + '/')),
      );
    }
  });
  useEffect(() => {
    if (previousFileOperations.current === finishedFileOperations) return;
    previousFileOperations.current = finishedFileOperations;
    void refreshFileOperations();
  }, [finishedFileOperations]);
  const label = (column: string) =>
    t(
      (
        {
          name: 'tools_filename',
          size: 'files_size',
          modified: 'files_modified',
          kind: 'files_type',
          permissions: 'files_permissions',
          owner: 'files_owner',
        } as const
      )[column as 'name'],
    );
  const ownership = (entry: RemoteEntry) =>
    `${entry.uid === null ? '—' : (names.get(`u:${entry.uid}`) ?? entry.uid)}/${entry.gid === null ? '—' : (names.get(`g:${entry.gid}`) ?? entry.gid)}`;
  const cell = (entry: RemoteEntry, column: string) =>
    column === 'name' ? (
      <span className="file-name">
        {entry.kind === 'directory' ? (
          <FolderIcon />
        ) : entry.kind === 'symlink' ? (
          <LinkIcon />
        ) : (
          <FileIcon />
        )}
        <TruncatedText className="truncate">{entry.name}</TruncatedText>
      </span>
    ) : column === 'size' ? (
      entry.kind === 'file' ? (
        fileSize(entry.size)
      ) : (
        '—'
      )
    ) : column === 'modified' ? (
      entry.modified === null ? (
        '—'
      ) : (
        new Date(entry.modified * 1000).toLocaleString()
      )
    ) : column === 'permissions' ? (
      permissions(entry)
    ) : column === 'owner' ? (
      ownership(entry)
    ) : (
      t(
        entry.kind === 'directory'
          ? 'files_directory'
          : entry.kind === 'symlink'
            ? 'files_link'
            : entry.kind === 'file'
              ? 'files_regular'
              : 'files_other',
      )
    );
  const showFileActions = width >= 600;
  const treeVisible = tree && (width >= 600 || narrowTree);
  const iconButton = (title: string, icon: React.ReactNode, action: () => void, off = false) => {
    return (
      <IconButton label={title} disabled={disabled || off} onClick={action}>
        {icon}
      </IconButton>
    );
  };
  return {
    t,
    service,
    pref,
    preferences,
    layout,
    treePanel,
    directory,
    typed,
    setTyped,
    editingPath,
    setEditingPath,
    loading,
    error,
    selection,
    setSelection,
    sort,
    setSort,
    filter,
    setFilter,
    hidden,
    columns,
    setNarrowTree,
    tree,
    width,
    treeRevision,
    setTreeRevision,
    names,
    edit,
    setEdit,
    property,
    setProperty,
    root,
    scroll,
    editNameId,
    editError,
    setEditError,
    editBusy,
    closeNameDialog,
    submitName,
    disabled,
    files,
    chosen,
    load,
    transfer,
    remove,
    showProperties,
    showPermissions,
    permissionEntries,
    setPermissionEntries,
    permissionError,
    startFileOperation,
    operationsOpen,
    setOperationsOpen,
    operationId,
    openEntry,
    label,
    ownership,
    cell,
    showFileActions,
    treeVisible,
    iconButton,
  };
}
