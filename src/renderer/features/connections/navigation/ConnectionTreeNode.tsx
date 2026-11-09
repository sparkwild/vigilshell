import { navigateMcp } from '../../mcp/model';

import { OperationIndicator } from '../OperationFeedback';

import {
  TreeItem,
  TreeItemContent,
  Collection,
  Button as AriaButton,
} from '../../../components/ui/data-tree';

import { ChevronRightIcon, TerminalIcon, GripVerticalIcon } from 'lucide-react';

import { cn } from 'cn';

import { Button } from '../../../components/ui/button';

import { ConnectionGroupHeader } from '../ConnectionGroupHeader';

import { EnvironmentBadge } from '../EnvironmentBadge';

import {
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
} from '../../../components/ui/context-menu';

import { TruncatedText } from '../../../components/ui/tooltip';

import { text } from '../text';

import { RowContextMenu } from './TreeContextMenu';

import { type Node } from './tree-node';

import type { useConnectionNavigation } from './use-connection-navigation';

export function renderConnectionNode(
  node: Node,
  model: ReturnType<typeof useConnectionNavigation>,
): React.ReactElement {
  const {
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
  } = model;
  const renderNode = (child: Node): React.ReactElement => renderConnectionNode(child, model);

  const c = node.connection;

  const session = c ? sessions.get(c.id) : undefined;

  return (
    <TreeItem
      id={node.id}
      textValue={node.name}
      aria-label={node.name}
      aria-description={
        c && c.environment !== 'none'
          ? text(locale, c.environment === 'test' ? 'environmentTest' : c.environment)
          : undefined
      }
      isDisabled={node.kind === 'notice'}
      className="connection-tree-item"
      data-kind={node.kind}
      data-placeholder={node.kind === 'emptyGroup' || node.kind === 'groupEnd' || undefined}
      data-in-group={(grouped && node.kind !== 'folder') || undefined}
      data-group-last={node.lastInGroup || undefined}
    >
      <TreeItemContent>
        {({ isExpanded, hasChildItems }) => (
          <RowContextMenu
            menu={
              c || node.kind === 'folder' ? (
                <ContextMenuGroup>
                  {c ? (
                    <>
                      <ContextMenuItem
                        disabled={
                          !state.available ||
                          c.deleting ||
                          (state.operations.isBusy('connection:' + c.id) &&
                            !state.store.isOpening(c.id) &&
                            !state.store.existingTerminal(c.id, 'establishing') &&
                            !state.store.existingTerminal(c.id, 'running'))
                        }
                        onClick={() => {
                          safely(
                            openConnection(c.id).then((sessionId) => {
                              if (sessionId) onConnected();
                            }),
                          );
                        }}
                      >
                        {translate('ws_connect')}
                      </ContextMenuItem>
                      <ContextMenuItem
                        disabled={
                          !state.available ||
                          c.deleting ||
                          state.operations.isBusy('connection:' + c.id)
                        }
                        onClick={async () => {
                          if (ui.beforeConnectionAction && !(await ui.beforeConnectionAction()))
                            return;
                          safely(state.store.connect(c.id, true));
                          onConnected();
                        }}
                      >
                        {translate('ws_connect_new')}
                      </ContextMenuItem>
                      <ContextMenuItem onClick={() => state.store.showTransfers(c.id)}>
                        {translate('files_view_transfers')}
                      </ContextMenuItem>
                      <ContextMenuSeparator />
                      <ContextMenuItem
                        disabled={state.operations.isBusy('connection:' + c.id)}
                        onClick={() => safely(state.store.editConnection(c.id))}
                      >
                        {t('edit')}
                      </ContextMenuItem>
                      <ContextMenuItem
                        disabled={c.deleting}
                        onClick={() => ui.showHostIdentity(c.id)}
                      >
                        {translate('tools_host_identity')}
                      </ContextMenuItem>
                      {c.credentialPending ? (
                        <ContextMenuItem
                          disabled={c.deleting}
                          onClick={() => ui.retryCredentialCleanup(c.id)}
                        >
                          {translate('tools_retry_cleanup')}
                        </ContextMenuItem>
                      ) : null}
                      <ContextMenuSeparator />
                      <ContextMenuItem
                        onClick={() => navigateMcp({ view: 'policy', connectionId: c.id })}
                      >
                        {translate('mcp_permissions')}
                      </ContextMenuItem>
                      <ContextMenuSeparator />
                      <ContextMenuItem
                        variant="destructive"
                        disabled={state.operations.isBusy('connection:' + c!.id)}
                        onClick={() => ui.deleteResource({ kind: 'connection', id: c.id })}
                      >
                        {c.deleting ? translate('ws_delete_retry') : t('delete')}
                      </ContextMenuItem>
                    </>
                  ) : node.kind === 'folder' ? (
                    <>
                      <ContextMenuItem onClick={() => state.showNew(node.folder?.id ?? null)}>
                        {t('new')}
                      </ContextMenuItem>
                      {node.folder ? (
                        <>
                          <ContextMenuSeparator />
                          <ContextMenuItem
                            disabled={!state.available || state.groupBusy}
                            onClick={() =>
                              state.setGroupEditing({
                                id: node.folder!.id,
                                surfaceId,
                                draft: node.folder!.name,
                                error: null,
                              })
                            }
                          >
                            {t('rename')}
                          </ContextMenuItem>
                          <ContextMenuSeparator />
                          <ContextMenuItem
                            variant="destructive"
                            disabled={!state.available || state.groupBusy}
                            onClick={() => safely(state.removeGroup(node.folder!))}
                          >
                            {t('removeGroup')}
                          </ContextMenuItem>
                        </>
                      ) : null}
                    </>
                  ) : null}
                </ContextMenuGroup>
              ) : null
            }
            finalFocus={
              state.groupEditing?.id === node.folder?.id && node.folder ? false : undefined
            }
          >
            <div className="connection-tree-row transition-colors">
              {node.kind === 'folder' ? (
                <ConnectionGroupHeader
                  folder={node.folder}
                  name={node.name}
                  expanded={isExpanded}
                  surfaceId={surfaceId}
                  count={
                    state.ready
                      ? state.connections.filter(
                          (c) => (c.groupId ?? null) === (node.folder?.id ?? null),
                        ).length
                      : null
                  }
                />
              ) : node.kind === 'emptyGroup' ? (
                <Button
                  variant="sidebar-placeholder"
                  size="sm"
                  className="connection-group-empty"
                  onClick={(event) => {
                    event.stopPropagation();
                    state.showNew(node.groupId ?? null);
                  }}
                >
                  {t('emptyGroupAction')}
                </Button>
              ) : node.kind === 'groupEnd' ? (
                <span className="connection-group-end">{t('dropAtEnd')}</span>
              ) : (
                <>
                  {hasChildItems ? (
                    <AriaButton slot="chevron" className="connection-chevron">
                      <ChevronRightIcon
                        className={isExpanded ? 'size-3.5 rotate-90' : 'size-3.5'}
                      />
                    </AriaButton>
                  ) : node.kind !== 'connection' ? (
                    <span className="connection-chevron" />
                  ) : null}
                  {node.kind === 'connection' ? (
                    <OperationIndicator
                      owner={'connection:' + c!.id}
                      relatedOwner={'connection-read:' + c!.id}
                      icon={
                        <TerminalIcon
                          className={cn('size-4', session && 'text-status-connected')}
                          aria-label={session ? t('connected') : undefined}
                        />
                      }
                      externalBusy={state.sessions.some(
                        (s) =>
                          s.connectionId === c!.id &&
                          ['connecting', 'authenticating', 'opening', 'trust'].includes(s.phase),
                      )}
                    />
                  ) : null}
                  <TruncatedText className="connection-tree-name">{node.name}</TruncatedText>
                  {c ? <EnvironmentBadge environment={c.environment} /> : null}
                </>
              )}
              {
                <AriaButton
                  appearance="sidebar-drag"
                  slot="drag"
                  aria-label={`${t('dragItem')} · ${node.name}`}
                  isDisabled={
                    !grouped ||
                    !state.available ||
                    state.groupBusy ||
                    state.groupEditing !== null ||
                    (!c && !node.folder)
                  }
                  className={
                    grouped && (c || node.folder)
                      ? 'connection-drag-handle transition-opacity'
                      : 'hidden'
                  }
                >
                  <GripVerticalIcon className="size-3" />
                </AriaButton>
              }
            </div>
          </RowContextMenu>
        )}
      </TreeItemContent>
      {node.children.length ? <Collection items={node.children}>{renderNode}</Collection> : null}
    </TreeItem>
  );
}
