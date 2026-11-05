import { useEffect, useRef, useState } from 'react';
import { ChevronRightIcon, FolderIcon, FileIcon, LinkIcon } from 'lucide-react';
import type { AgentFileEntry, AgentFilesPage } from '@contracts';
import {
  Tree,
  TreeItem,
  TreeItemContent,
  Collection,
  Button as TreeButton,
  type Key,
} from '../../../components/ui/data-tree';
import { Button } from '../../../components/ui/button';
import { Checkbox } from '../../../components/ui/checkbox';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TruncatedText,
} from '../../../components/ui/tooltip';
import { Spinner } from '../../../components/ui/spinner';
import { Empty, EmptyHeader, EmptyDescription } from '../../../components/ui/empty';
import { useDesktop } from '../../../app/DesktopProvider';
import type { PickedResource } from './PolicyResourcePicker';
type Branch = { page?: AgentFilesPage; pending?: boolean; error?: boolean };
export type ResourceNode = {
  id: string;
  name: string;
  kind: string;
  children?: ResourceNode[];
  notice?: 'loading' | 'error' | 'more' | 'empty';
  parent?: string;
};
export function buildResourceTree(
  branches: Map<string, Branch>,
  query: string,
): ResourceNode | null {
  const visit = (entry: ResourceNode): ResourceNode | null => {
    const branch = branches.get(entry.id);
    const children =
      entry.kind === 'directory'
        ? (branch?.page?.entries ?? [])
            .filter(
              (e) =>
                e.path !== entry.id && e.path.startsWith(entry.id === '/' ? '/' : entry.id + '/'),
            )
            .map((e) => visit({ id: e.path, name: e.name, kind: e.kind }))
            .filter((e): e is ResourceNode => !!e)
        : [];
    const matches = entry.name.toLocaleLowerCase().includes(query.toLocaleLowerCase());
    if (query && !matches && !children.length) return null;
    if (entry.kind === 'directory') {
      const notice = branch?.pending
        ? 'loading'
        : branch?.error
          ? 'error'
          : branch?.page?.nextCursor
            ? 'more'
            : !branch?.page
              ? 'loading'
              : !branch.page.entries.length
                ? 'empty'
                : null;
      if (notice)
        children.push({
          id: entry.id + '\0' + notice,
          name: notice,
          kind: 'notice',
          notice,
          parent: entry.id,
        });
      return { ...entry, children };
    }
    return entry;
  };
  return visit({ id: '/', name: '/', kind: 'directory' });
}
export function PolicyResourceTree({
  read,
  selection,
  toggle,
  search,
  saving,
  target,
  refresh,
  onNavigate,
  onBusy,
}: {
  read(path: string, after: string | null): Promise<AgentFilesPage | null>;
  selection: PickedResource[];
  toggle(value: PickedResource, checked: boolean): void;
  search: string;
  saving: boolean;
  target: { path: string; version: number };
  refresh: number;
  onNavigate(path: string): void;
  onBusy(busy: boolean): void;
}) {
  const { t } = useDesktop();
  const [branches, setBranches] = useState(new Map<string, Branch>()),
    [expanded, setExpanded] = useState<Set<Key>>(new Set(['/']));
  const state = useRef(branches);
  state.current = branches;
  const requests = useRef(new Map<string, Promise<AgentFilesPage | null>>()),
    alive = useRef(true),
    navigation = useRef(0);
  const readRef = useRef(read);
  readRef.current = read;
  const load = async (
    path: string,
    after: string | null = null,
    force = false,
  ): Promise<AgentFilesPage | null> => {
    if (requests.current.has(path)) return requests.current.get(path)!;
    const existing = state.current.get(path);
    if (!force && !after && existing?.page) return existing.page;
    setBranches((old) => new Map(old).set(path, { ...old.get(path), pending: true, error: false }));
    const pending = readRef
      .current(path, after)
      .catch(() => null)
      .then((page) => {
        if (alive.current)
          setBranches((old) => {
            const prior = old.get(path)?.page;
            return new Map(old).set(path, {
              page: page
                ? after && prior
                  ? {
                      ...page,
                      entries: [
                        ...new Map(
                          [...prior.entries, ...page.entries].map((e) => [e.path, e]),
                        ).values(),
                      ],
                    }
                  : page
                : prior,
              error: !page,
              pending: false,
            });
          });
        return page;
      })
      .finally(() => requests.current.delete(path));
    requests.current.set(path, pending);
    return pending;
  };
  const pendingFocus = useRef<string | null>(null);
  const location = useRef('/');
  const loader = useRef(load);
  loader.current = load;
  useEffect(() => {
    const currentNavigation = navigation;
    alive.current = true;
    void loader.current('/');
    return () => {
      alive.current = false;
      currentNavigation.current++;
    };
  }, []);
  useEffect(() => {
    onBusy([...branches.values()].some((b) => b.pending));
  }, [branches, onBusy]);
  useEffect(() => {
    for (const key of expanded)
      if (!state.current.has(String(key))) void loader.current(String(key));
  }, [expanded]);
  useEffect(() => {
    if (!target.version) return;
    const generation = ++navigation.current;
    void (async () => {
      const parts = target.path.split('/').filter(Boolean);
      let parent = '/';
      const ancestors: string[] = ['/'];
      let found: AgentFileEntry | undefined;
      for (const part of parts) {
        let page = await loader.current(parent);
        const desired = (parent === '/' ? '' : parent) + '/' + part;
        while (page && !page.entries.some((e) => e.path === desired) && page.nextCursor) {
          page = await loader.current(parent, page.nextCursor, true);
          if (generation !== navigation.current) return;
        }
        if (generation !== navigation.current || !alive.current) return;
        found = page?.entries.find((e) => e.path === desired);
        if (!found || (found.kind !== 'directory' && desired !== target.path)) {
          setBranches((old) => new Map(old).set(parent, { ...old.get(parent), error: true }));
          break;
        }
        if (found.kind === 'directory') {
          parent = desired;
          ancestors.push(parent);
        }
      }
      if (generation !== navigation.current || !alive.current) return;
      location.current = parent;
      onNavigate(parent);
      pendingFocus.current = found?.path ?? parent;
      setExpanded((old) => new Set([...old, ...ancestors]));
    })();
  }, [target, onNavigate]);
  useEffect(() => {
    if (refresh) void loader.current(location.current, null, true);
  }, [refresh]);
  const root = buildResourceTree(branches, search);
  const filteredExpanded = new Set<Key>(expanded);
  const expandMatches = (n: ResourceNode) => {
    if (n.children?.length) {
      filteredExpanded.add(n.id);
      n.children.forEach(expandMatches);
    }
  };
  if (search && root) expandMatches(root);
  const render = (node: ResourceNode): React.ReactElement => {
    if (node.notice)
      return (
        <TreeItem
          id={node.id}
          textValue={t(
            node.notice === 'error'
              ? 'files_tree_error'
              : node.notice === 'more'
                ? 'policy_more_entries'
                : node.notice === 'empty'
                  ? 'policy_picker_empty'
                  : 'ws_loading',
          )}
          className="file-tree-item"
        >
          <TreeItemContent>
            <div className="flex min-w-0 items-center gap-2">
              {node.notice === 'loading' ? (
                <>
                  <Spinner />
                  <span>{t('ws_loading')}</span>
                </>
              ) : node.notice === 'empty' ? (
                <span>{t('policy_picker_empty')}</span>
              ) : (
                <Button
                  size="xs"
                  variant="ghost"
                  disabled={saving}
                  onClick={() =>
                    void loader.current(
                      node.parent!,
                      node.notice === 'more'
                        ? (branches.get(node.parent!)?.page?.nextCursor ?? null)
                        : null,
                      true,
                    )
                  }
                >
                  {t(node.notice === 'more' ? 'policy_more_entries' : 'retry')}
                </Button>
              )}
            </div>
          </TreeItemContent>
        </TreeItem>
      );
    const supported = node.kind === 'file' || node.kind === 'directory',
      checked = selection.some((s) => s.path === node.id);
    const covered = selection.some(
      (s) =>
        s.kind === 'directory' &&
        s.path !== node.id &&
        (s.path === '/' || node.id.startsWith(s.path + '/')),
    );
    const reason = !supported
      ? t('policy_picker_unsupported')
      : covered
        ? t('policy_tree_covered')
        : !checked && selection.length >= 128
          ? t('policy_picker_limit')
          : null;
    return (
      <TreeItem
        id={node.id}
        textValue={node.name}
        className="file-tree-item"
        isDisabled={!supported}
      >
        <TreeItemContent>
          {({ isExpanded, hasChildItems }) => (
            <>
              <TreeButton
                slot="chevron"
                aria-label={node.name}
                className="file-tree-chevron"
                isDisabled={saving}
              >
                {hasChildItems ? (
                  <ChevronRightIcon
                    size={12}
                    style={{ transform: isExpanded ? 'rotate(90deg)' : undefined }}
                  />
                ) : null}
              </TreeButton>
              <Tooltip disabled={!reason} trackCursorAxis="both">
                <TooltipTrigger
                  render={<span className="flex min-w-0 flex-1 items-center gap-2" />}
                  id={'policy-resource-' + node.id}
                  ref={(element: HTMLElement | null) => {
                    if (!element || pendingFocus.current !== node.id) return;
                    pendingFocus.current = null;
                    const generation = navigation.current;
                    requestAnimationFrame(() => {
                      if (
                        !alive.current ||
                        generation !== navigation.current ||
                        !element.isConnected
                      )
                        return;
                      element.scrollIntoView?.({ block: 'nearest' });
                      (element.closest<HTMLElement>('[role="row"]') ?? element).focus();
                    });
                  }}
                  tabIndex={reason ? 0 : -1}
                >
                  <Checkbox
                    aria-label={node.name}
                    checked={checked || covered}
                    disabled={saving || !!reason}
                    onCheckedChange={(on) => {
                      if (supported && !reason)
                        toggle({ path: node.id, kind: node.kind as PickedResource['kind'] }, on);
                    }}
                  />
                  <Button
                    variant="ghost"
                    size="xs"
                    className="min-w-0 flex-1 justify-start"
                    disabled={saving || !supported}
                    onClick={() => {
                      navigation.current++;
                      location.current =
                        node.kind === 'directory'
                          ? node.id
                          : node.id.slice(0, node.id.lastIndexOf('/')) || '/';
                      onNavigate(location.current);
                      if (node.kind === 'directory')
                        setExpanded((old) => {
                          const next = new Set(old);
                          if (next.has(node.id)) next.delete(node.id);
                          else next.add(node.id);
                          return next;
                        });
                    }}
                  >
                    {node.kind === 'directory' ? (
                      <FolderIcon data-icon="inline-start" />
                    ) : node.kind === 'symlink' ? (
                      <LinkIcon data-icon="inline-start" />
                    ) : (
                      <FileIcon data-icon="inline-start" />
                    )}
                    <TruncatedText>{node.name}</TruncatedText>
                  </Button>
                </TooltipTrigger>
                {reason ? <TooltipContent>{reason}</TooltipContent> : null}
              </Tooltip>
            </>
          )}
        </TreeItemContent>
        {node.children ? <Collection items={node.children}>{render}</Collection> : null}
      </TreeItem>
    );
  };
  return root ? (
    <Tree
      aria-label={t('policy_choose_server_paths')}
      selectionMode="none"
      expandedKeys={search ? filteredExpanded : expanded}
      onExpandedChange={(keys) => setExpanded(new Set(keys))}
      items={[root]}
    >
      {render}
    </Tree>
  ) : (
    <Empty>
      <EmptyHeader>
        <EmptyDescription>{t('policy_picker_no_match')}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}
