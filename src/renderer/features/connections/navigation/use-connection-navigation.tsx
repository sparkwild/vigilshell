import { useOpenConnection } from '../ui-context';

import { useDesktop } from '../../../app/DesktopProvider';

import { useMemo, useId, useRef, useState, useEffect } from 'react';

import { useDragAndDrop, DropIndicator, type DropTarget } from '../../../components/ui/data-tree';

import { layoutAfterDrop } from '../group-layout';

import { useNavigationInteraction } from '../../../components/ui/navigation-interaction';

import type { GroupLayout } from '@contracts';

import { visibleConnections, connectionGroups, hasSearchOrFilters } from '../navigation';

import type { ConnectionSummary } from '@contracts';

import { useWorkspaceUi } from '../ui-context';

import { useConnections } from '../ConnectionProvider';

import { text } from '../text';

import { type Node } from './tree-node';

export function useConnectionNavigation({ onConnected }: { onConnected(): void }) {
  const openConnection = useOpenConnection();

  const ui = useWorkspaceUi();

  const { t: translate } = useDesktop();

  const state = useConnections();

  const surfaceId = useId();

  const [dragKey, setDragKey] = useState<string | null>(null);

  const dragRef = useRef<string | null>(null);

  const dragLayout = useRef<GroupLayout | null>(null);

  const [dropping, setDropping] = useState(false);

  const [hoverGroup, setHoverGroup] = useState<string | null>(null);

  const grouped = state.navigation.view === 'groups';

  const { groupExpanded, setGroupExpanded } = state;

  useEffect(() => {
    if (!dragKey || !hoverGroup || groupExpanded.has(hoverGroup)) return;
    // 基于组件库已确认的目标计时，避免原生 dragover 激活回调捕获上一个目标。
    const timer = window.setTimeout(() => {
      setGroupExpanded((previous) => new Set(previous).add(hoverGroup));
    }, 600);
    return () => window.clearTimeout(timer);
  }, [dragKey, hoverGroup, groupExpanded, setGroupExpanded]);

  useNavigationInteraction(
    grouped &&
      (dragKey !== null ||
        dropping ||
        state.groupEditing?.surfaceId === surfaceId ||
        state.interactionSurface === surfaceId),
  );

  const allowedDrop = (target: DropTarget) =>
    !!(
      target.type === 'item' &&
      dragRef.current &&
      dragLayout.current &&
      layoutAfterDrop(dragLayout.current, dragRef.current, target)
    );

  const { dragAndDropHooks } = useDragAndDrop<Node>({
    // 保存开始后仍保留本次拖拽的监听，原生 dragend 才能完成组件库的清理。
    isDisabled:
      !grouped ||
      !state.available ||
      (state.groupBusy && dragKey === null) ||
      state.groupEditing !== null,
    getItems(keys) {
      return [...keys]
        .filter((key) => String(key).startsWith('c:') || String(key).startsWith('f:'))
        .map((key) => ({ 'application/x-vigilshell-navigation': String(key) }));
    },
    acceptedDragTypes: ['application/x-vigilshell-navigation'],
    getAllowedDropOperations: () => ['move'],
    getDropOperation: (target, types) =>
      types.has('application/x-vigilshell-navigation') && allowedDrop(target) ? 'move' : 'cancel',
    onDragStart(event) {
      const key = String([...event.keys][0]);
      dragRef.current = key;
      dragLayout.current = state.currentLayout();
      setDragKey(key);
    },
    onDragEnd() {
      dragRef.current = null;
      setDragKey(null);
      setHoverGroup(null);
    },
    onDropEnter(event) {
      const key =
        event.target.type === 'item' && event.target.dropPosition === 'on'
          ? String(event.target.key)
          : null;
      setHoverGroup(key && (key.startsWith('f:') || key === 'u:root') ? key : null);
    },
    onDropExit() {
      setHoverGroup(null);
    },
    onDropActivate(event) {
      if (event.target.type === 'item') {
        const key = String(event.target.key);
        if (key.startsWith('f:') || key === 'u:root')
          state.setGroupExpanded((previous) => new Set(previous).add(key));
      }
    },
    onMove(event) {
      if (event.keys.size !== 1) return;
      const layout = layoutAfterDrop(
        dragLayout.current ?? state.currentLayout(),
        [...event.keys][0]!,
        event.target,
      );
      if (!layout) return;
      setDropping(true);
      void state
        .commitLayout(layout)
        .catch(() => undefined)
        .finally(() => setDropping(false));
    },
    renderDropIndicator: (target) => (
      <DropIndicator target={target} className="connection-drop-indicator" />
    ),
  });

  const locale = state.snapshot.appearance.locale;

  const t = (key: Parameters<typeof text>[1]) => text(locale, key);

  const sessions = new Map(
    state.connections.map((c) => [
      c.id,
      state.sessions.find((s) => s.connectionId === c.id && s.phase === 'running'),
    ]),
  );

  const visible = useMemo(
    () => visibleConnections(state.connections, state.sessions, state.navigation, locale),
    [state.connections, state.sessions, state.navigation, locale],
  );

  function connectionNode(c: ConnectionSummary): Node {
    return {
      id: 'c:' + c.id,
      name: c.name,
      kind: 'connection',
      connection: c,
      children: [],
    };
  }

  const groups = connectionGroups(
    state.folders,
    visible,
    state.ready && hasSearchOrFilters(state.navigation) && dragKey === null,
    locale,
  );

  if (grouped && dragKey && !groups.some((group) => group.id === 'u:root'))
    groups.push({ id: 'u:root', connections: [] });

  const nodes: Node[] = !grouped
    ? visible.map(connectionNode)
    : groups.map((group) => {
        const groupId = group.folder?.id ?? null;
        const children = [...group.connections]
          .sort((a, b) => a.groupSortOrder - b.groupSortOrder || a.id.localeCompare(b.id))
          .map((connection) => ({ ...connectionNode(connection), groupId }));
        const special: Node[] = !state.ready
          ? [
              {
                id: 'locked:' + (groupId ?? 'ungrouped'),
                name: translate('ws_connections_locked'),
                kind: 'notice',
                groupId,
                children: [],
              },
            ]
          : children.length
            ? []
            : [
                {
                  id: 'empty:' + (groupId ?? 'ungrouped'),
                  name: t('emptyGroupAction'),
                  kind: 'emptyGroup',
                  groupId,
                  children: [],
                },
              ];
        if (dragKey && children.length)
          special.push({
            id: 'end:' + (groupId ?? 'ungrouped'),
            name: t('dropAtEnd'),
            kind: 'groupEnd',
            groupId,
            children: [],
          });
        return {
          id: group.id,
          name: group.folder?.name ?? t('root'),
          kind: 'folder',
          folder: group.folder,
          children: [...children, ...special].map((child, index, items) => ({
            ...child,
            lastInGroup: index === items.length - 1,
          })),
        };
      });

  const expanded = new Set([...(state.navigation.view === 'groups' ? state.groupExpanded : [])]);

  function changeExpanded(keys: Set<import('../../../components/ui/data-tree').Key>) {
    const next = new Set([...keys].map(String));
    const visibleGroups = new Set(
      state.navigation.view === 'groups' ? groups.map((g) => g.id) : [],
    );
    const changedGroup = [...visibleGroups].some(
      (key) => state.groupExpanded.has(key) !== next.has(key),
    );
    if (changedGroup)
      state.setGroupExpanded(
        (previous) =>
          new Set(
            [...previous]
              .filter((k) => !visibleGroups.has(k))
              .concat([...next].filter((k) => visibleGroups.has(k))),
          ),
      );
  }

  const safely = (p: Promise<unknown>) => {
    void p.catch(() => undefined);
  };

  function activate(key: string) {
    const update = (keys: Set<string>) => {
      const next = new Set(keys);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    };
    if (key.startsWith('c:')) {
      void openConnection(key.slice(2)).then((sessionId) => {
        if (sessionId) onConnected();
      });
    } else if (key.startsWith('f:') || key === 'u:root') state.setGroupExpanded(update);
  }
  return {
    sessions,
    state,
    locale,
    grouped,
    translate,
    safely,
    t,
    openConnection,
    onConnected,
    ui,
    surfaceId,
    expanded,
    nodes,
    dragAndDropHooks,
    dragKey,
    changeExpanded,
    activate,
    visible,
  };
}
