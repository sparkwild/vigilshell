import { useEffect, useRef, useState } from 'react';
import {
  Tree,
  Virtualizer,
  ListLayout,
  TreeItem,
  TreeItemContent,
  Collection,
  Button as TreeButton,
  type Key,
} from '../../components/ui/data-tree';
import { ChevronRightIcon, FolderIcon } from 'lucide-react';
import type { RemoteEntry, TerminalSessionView } from '@contracts';
import { useWorkspace } from '../connections/workspace-context';
import { useDesktop } from '../../app/DesktopProvider';
import { directories } from './directory-service';
type Node = { entry?: RemoteEntry; id: string; name: string; children?: Node[]; notice?: boolean };
export function FileDirectoryTree({
  session,
  online,
  currentPath,
  navigate,
  contextPath,
  onContextMenu,
}: {
  session: TerminalSessionView;
  online: boolean;
  currentPath: string;
  navigate(path: string): void;
  contextPath?: string;
  onContextMenu?(entry: RemoteEntry | null): void;
}) {
  const { store } = useWorkspace(),
    { t } = useDesktop(),
    service = directories(store);
  const pref = service.preference(session.connectionId);
  const [expanded, setExpanded] = useState<Set<Key>>(new Set(pref.expanded));
  const [nodes, setNodes] = useState(new Map<string, Node[]>());
  const requests = useRef(new Map<string, AbortController>());
  useEffect(() => {
    if (!online) return;
    for (const path of expanded) {
      const key = String(path);
      if (nodes.has(key) || requests.current.has(key)) continue;
      const abort = new AbortController();
      requests.current.set(key, abort);
      void service
        .read(session, key, abort.signal, 1)
        .then((d) => {
          if (!abort.signal.aborted)
            setNodes((old) => {
              const children = d.entries
                .filter((e) => e.kind === 'directory')
                .map((e) => ({ id: e.path, name: e.name, entry: e }));
              const count = [...old]
                .filter(([id]) => id !== key)
                .reduce((n, [, values]) => n + values.length, 0);
              return new Map(old).set(
                key,
                count + children.length > 20000
                  ? [
                      {
                        id: `${key}\0limit`,
                        name: t('files_tree_limit'),
                        notice: true,
                      },
                    ]
                  : children,
              );
            });
        })
        .catch(() => {
          if (!abort.signal.aborted)
            setNodes((old) =>
              new Map(old).set(key, [
                {
                  id: `${key}\0error`,
                  name: t('files_tree_error'),
                  notice: true,
                },
              ]),
            );
        })
        .finally(() => {
          if (requests.current.get(key) === abort) requests.current.delete(key);
        });
    }
    for (const [path, request] of requests.current)
      if (!expanded.has(path)) {
        request.abort();
        requests.current.delete(path);
      }
  }, [expanded, nodes, online, service, session, t]);
  useEffect(
    () => () => {
      for (const request of requests.current.values()) request.abort();
    },
    [],
  );
  function hydrate(node: Node): Node {
    if (node.notice || !online) return node;
    return {
      ...node,
      children: (
        nodes.get(node.id) ?? [
          { id: `${node.id}\0loading`, name: t('tools_loading'), notice: true },
        ]
      ).map(hydrate),
    };
  }
  function render(node: Node): React.ReactElement {
    return (
      <TreeItem
        id={node.id}
        textValue={node.name}
        isDisabled={node.notice || !online}
        className="file-tree-item"
        onContextMenu={() => {
          if (node.notice || !online) return;
          onContextMenu?.(node.entry ?? null);
        }}
      >
        <TreeItemContent>
          {({ isExpanded, hasChildItems }) => (
            <div className="file-tree-row">
              <TreeButton slot="chevron" aria-label={node.name} className="file-tree-chevron">
                {hasChildItems ? (
                  <ChevronRightIcon
                    size={12}
                    style={{
                      transform: isExpanded ? 'rotate(90deg)' : undefined,
                    }}
                  />
                ) : null}
              </TreeButton>
              <FolderIcon size={14} />
              <span className="truncate">{node.name}</span>
            </div>
          )}
        </TreeItemContent>
        {node.children ? <Collection items={node.children}>{render}</Collection> : null}
      </TreeItem>
    );
  }
  return (
    <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: 28 }}>
      <Tree
        aria-label={t('files_directory_tree')}
        selectionMode="single"
        selectionBehavior="replace"
        disallowEmptySelection
        selectedKeys={new Set([contextPath ?? currentPath])}
        expandedKeys={expanded}
        onExpandedChange={(keys) => {
          if (keys.size > 32) {
            store.report('files_too_large');
            return;
          }
          setNodes((old) => {
            const next = new Map(old);
            for (const path of next.keys()) if (!keys.has(path)) next.delete(path);
            return next;
          });
          setExpanded(new Set(keys));
          pref.expanded = new Set([...keys].map(String));
          service.persist(session);
        }}
        onSelectionChange={(keys) => {
          if (keys === 'all') return;
          const key = keys.values().next().value;
          if (key !== undefined && !String(key).includes('\0')) navigate(String(key));
        }}
        items={[hydrate({ id: '/', name: '/' })]}
      >
        {render}
      </Tree>
    </Virtualizer>
  );
}
