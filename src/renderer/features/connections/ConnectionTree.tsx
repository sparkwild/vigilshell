import { Alert, AlertDescription } from '../../components/ui/alert';
import { errorText } from './text';
import { ConnectionUnlock } from './ConnectionUnlock';
import { Tree } from '../../components/ui/data-tree';

import { Button } from '../../components/ui/button';

import { Tabs, TabsContent } from '../../components/ui/tabs';

import { Empty, EmptyHeader, EmptyDescription } from '../../components/ui/empty';

import { ConnectionNavigationToolbar } from './ConnectionNavigationToolbar';

import { type Node } from './navigation/tree-node';

import { useConnectionNavigation } from './navigation/use-connection-navigation';

import { renderConnectionNode } from './navigation/ConnectionTreeNode';

export function ConnectionTree({
  onConnected,
  showSelection = true,
}: {
  onConnected(): void;
  showSelection?: boolean;
}) {
  const model = useConnectionNavigation({ onConnected });
  const {
    t,
    state,
    surfaceId,
    nodes,
    dragAndDropHooks,
    dragKey,
    safely,
    expanded,
    changeExpanded,
    activate,
    visible,
  } = model;
  const renderNode = (node: Node) => renderConnectionNode(node, model);
  return (
    <Tabs
      render={<section />}
      className="connection-navigation"
      aria-label={t('connections')}
      value={state.navigation.view}
      onValueChange={(view) => {
        if (view !== 'groups') state.setGroupEditing(null);
        if (view === 'connections' || view === 'groups')
          state.setNavigation((previous) => ({ ...previous, view }));
      }}
    >
      <ConnectionNavigationToolbar surfaceId={surfaceId} />
      <TabsContent
        value={state.navigation.view}
        className="flex min-h-0 flex-1 flex-col gap-2"
        keepMounted
      >
        <ConnectionUnlock />
        {state.navigation.view === 'groups' && state.groupsError ? (
          <Alert variant="destructive">
            <AlertDescription>
              {errorText(state.snapshot.appearance.locale, state.groupsError)}
            </AlertDescription>
          </Alert>
        ) : null}
        <Tree
          aria-label={t('connections')}
          items={nodes}
          dragAndDropHooks={dragAndDropHooks}
          data-drag-active={dragKey !== null || undefined}
          selectionMode="single"
          selectionBehavior="replace"
          selectedKeys={showSelection && state.selected ? ['c:' + state.selected.id] : []}
          onSelectionChange={(keys) => {
            if (keys !== 'all') {
              const key = String(keys.values().next().value ?? '');
              if (key.startsWith('c:')) {
                safely(state.store.select({ kind: 'connection', id: key.slice(2) }));
              }
            }
          }}
          expandedKeys={expanded}
          onExpandedChange={changeExpanded}
          onAction={(key) => activate(String(key))}
          className={nodes.length ? 'connection-tree' : 'connection-tree connection-tree-hidden'}
        >
          {renderNode}
        </Tree>
        {!state.loading &&
        !state.error &&
        state.available &&
        state.connections.length > 0 &&
        !visible.length ? (
          <Empty className="connection-tree-empty">
            <EmptyHeader>
              <EmptyDescription>{t('noMatches')}</EmptyDescription>
            </EmptyHeader>
            <Button
              size="sm"
              variant="outline"
              onClick={() =>
                state.setNavigation((previous) => ({
                  ...previous,
                  search: '',
                  ...(previous.view === 'connections' ? { status: 'all' as const } : {}),
                }))
              }
            >
              {t(state.navigation.view === 'groups' ? 'clearSearch' : 'clearSearchFilters')}
            </Button>
          </Empty>
        ) : null}
      </TabsContent>
    </Tabs>
  );
}

export { useConnectionNavigation } from './navigation/use-connection-navigation';

export { renderConnectionNode } from './navigation/ConnectionTreeNode';
