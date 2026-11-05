import { UiStateFeedback } from '../settings/UiStateFeedback';
import { FilePermissionsDialog } from './FilePermissionsDialog';
import { FileOperationsDialog } from './FileOperationsDialog';
import { FileContextMenu } from './FileContextMenu';
import { fileMenuTarget, type FileMenuTarget } from './file-menu-target';
import { useWorkspace } from '../connections/workspace-context';
import { useLayoutEffect, useState } from 'react';
import {
  Empty,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  EmptyDescription,
  EmptyContent,
} from '../../components/ui/empty';
import { Spinner } from '../../components/ui/spinner';
import { Field, FieldLabel, FieldError } from '../../components/ui/field';

import { PathInput } from '../../components/ui/path-input';

import { IconButton } from '../../components/ui/icon-button';

import { fileColumns } from '../../../shared/preferences';

import { Tooltip, TooltipTrigger, TooltipContent } from '../../components/ui/tooltip';

import type { AgentTask, TerminalSessionView } from '@contracts';
import { AgentFileFeedback } from './AgentFileFeedback';

import {
  Table,
  TableHeader,
  TableBody,
  Column,
  Row,
  Cell,
  Virtualizer,
  TableLayout,
  ResizableTableContainer,
  ColumnResizer,
} from '../../components/ui/data-table';

import {
  FolderOpenIcon,
  ArrowUpIcon,
  ArrowLeftIcon,
  ArrowRightIcon,
  HomeIcon,
  RefreshCwIcon,
  UploadIcon,
  DownloadIcon,
  FolderPlusIcon,
  MoreHorizontalIcon,
  PanelLeftOpenIcon,
  PanelLeftCloseIcon,
  SearchIcon,
  EyeOffIcon,
  CircleAlertIcon,
} from 'lucide-react';

import { errorKey } from '../connections/workspace-context';

import { Button } from '../../components/ui/button';

import { Input } from '../../components/ui/input';

import {
  Dialog,
  FormDialog,
  DialogForm,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '../../components/ui/dialog';

import { ContextMenu, ContextMenuTrigger } from '../../components/ui/context-menu';

import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuItem,
  DropdownMenuGroup,
  DropdownMenuSeparator,
} from '../../components/ui/dropdown-menu';

import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '../../components/ui/resizable';

import { FileDirectoryTree } from './FileDirectoryTree';

import { FileDirectoryPanel } from './FileDirectoryPanel';

import { fileSize, parentPath, permissions } from './file-format';

import { useFileBrowser } from './model/use-file-browser';

export { transferActive, parentPath, validFileName } from './file-format';

export { TransferList } from './TransferPanel';

export function FileBrowser({
  session,
  online,
  visible,
  panelActions,
  agentTasks,
  agentError = null,
}: {
  session: TerminalSessionView;
  online: boolean;
  visible: boolean;
  panelActions?: React.ReactNode;
  agentTasks?: AgentTask[];
  agentError?: string | null;
}) {
  const model = useFileBrowser({ session, online, visible, panelActions, agentTasks });
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [contextOpen, setContextOpen] = useState(false);
  const [menu, setMenu] = useState<FileMenuTarget | null>(null);
  const { state: workspace } = useWorkspace();
  useLayoutEffect(() => {
    if (!online) {
      setOptionsOpen(false);
      setContextOpen(false);
    }
  }, [online]);
  const {
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
  } = model;

  const menuValid =
    !!menu && menu.runtimeId === workspace.runtimeId && menu.groupId === session.groupId;
  const menuContent = (
    <FileContextMenu
      target={menuValid ? menu : null}
      disabled={disabled || !menuValid}
      refresh={() => {
        if (!menuValid) return;
        service.invalidate(session.groupId, menu.directory);
        setTreeRevision((n) => n + 1);
        if (menu.source !== 'tree' || menu.directory === directory.path)
          void load(directory.path, undefined, true);
      }}
      open={(entry, alternative) => {
        if (menuValid) void openEntry(entry, alternative);
      }}
      upload={(path) => {
        if (menuValid) void transfer('upload', [], path);
      }}
      download={(entries) => {
        if (menuValid) void transfer('download', entries);
      }}
      archive={(paths) => {
        if (menuValid) void transfer('download', paths, directory.path, true);
      }}
      create={(path, kind) => {
        if (menuValid) {
          setEditError(null);
          setEdit({ entry: null, name: '', directory: path, kind });
        }
      }}
      rename={(entry) => {
        if (menuValid) {
          setEditError(null);
          setEdit({ entry, name: entry.name });
        }
      }}
      remove={(entry) => {
        if (menuValid) void remove(entry);
      }}
      properties={(path) => {
        if (menuValid) void showProperties(path);
      }}
      permissions={(paths) => {
        if (menuValid) void showPermissions(paths);
      }}
    />
  );

  const directoryError = error && error !== 'restored_path_unavailable' ? error : null;
  const hiddenOnly = directory.entries.length > 0 && !filter;
  const emptyTitle = loading
    ? t('tools_loading')
    : directoryError
      ? t('files_directory_read_failed')
      : filter
        ? t('files_no_matches')
        : hiddenOnly
          ? t('files_no_visible_entries')
          : t('tools_empty_directory');

  return (
    <div ref={root} className="file-browser">
      <UiStateFeedback scope={`connection:${session.connectionId}`} />
      <AgentFileFeedback tasks={agentTasks ?? []} error={agentError} />
      <div className="file-toolbar" role="group" aria-label={t('tools_files')}>
        <div className="file-toolbar-interactions" inert={!online}>
          <Tooltip>
            <TooltipTrigger
              render={
                <span className="file-toolbar-label" tabIndex={0} aria-label={t('tools_files')} />
              }
            >
              <FolderOpenIcon size={16} />
            </TooltipTrigger>
            <TooltipContent>{t('tools_files')}</TooltipContent>
          </Tooltip>
          {iconButton(
            t('files_back'),
            <ArrowLeftIcon />,
            () => void load(pref.history[pref.index - 1]!, pref.index - 1),
            pref.index <= 0,
          )}
          {iconButton(
            t('files_forward'),
            <ArrowRightIcon />,
            () => void load(pref.history[pref.index + 1]!, pref.index + 1),
            pref.index >= pref.history.length - 1,
          )}
          {iconButton(
            t('tools_parent'),
            <ArrowUpIcon />,
            () => void load(parentPath(directory.path)),
            false,
          )}
          {iconButton(t('files_home'), <HomeIcon />, () => void load('.'), false)}
          <PathInput
            path={directory.path}
            value={typed}
            editing={editingPath}
            disabled={disabled}
            label={t('tools_path')}
            editLabel={t('files_edit_path')}
            onValueChange={setTyped}
            onEditingChange={setEditingPath}
            onNavigate={(path) => void load(path)}
          />
          {iconButton(
            t('tools_refresh'),
            <RefreshCwIcon />,
            () => {
              service.invalidate(session.groupId);
              setTreeRevision((v) => v + 1);
              void load(directory.path, undefined, true);
            },
            false,
          )}
          {showFileActions ? (
            <>
              {iconButton(t('tools_upload'), <UploadIcon />, () => void transfer('upload'), false)}
              {iconButton(
                t('tools_download'),
                <DownloadIcon />,
                () => void transfer('download'),
                !chosen.length || chosen.some((e) => !['file', 'directory'].includes(e.kind)),
              )}
              {iconButton(
                t('tools_mkdir'),
                <FolderPlusIcon />,
                () => (setEditError(null), setEdit({ entry: null, name: '' })),
              )}
            </>
          ) : null}
          <Button variant="ghost" size="sm" onClick={() => setOperationsOpen(true)}>
            {t('files_operations')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void window.vigilshell.fileEditors.request({ action: 'show' })}
          >
            {t('files_editing')}
          </Button>
          <DropdownMenu open={online && optionsOpen} onOpenChange={setOptionsOpen}>
            <Tooltip>
              <DropdownMenuTrigger
                render={
                  <TooltipTrigger
                    render={
                      <Button variant="ghost" size="icon-sm" aria-label={t('files_options')} />
                    }
                  />
                }
              >
                <MoreHorizontalIcon />
              </DropdownMenuTrigger>
              <TooltipContent>{t('files_options')}</TooltipContent>
            </Tooltip>
            <DropdownMenuContent>
              <DropdownMenuGroup>
                {!showFileActions ? (
                  <>
                    <DropdownMenuItem disabled={disabled} onClick={() => void transfer('upload')}>
                      {t('tools_upload')}
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      disabled={
                        disabled ||
                        !chosen.length ||
                        chosen.some((e) => !['file', 'directory'].includes(e.kind))
                      }
                      onClick={() => void transfer('download')}
                    >
                      {t('tools_download')}
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      disabled={disabled}
                      onClick={() => (setEditError(null), setEdit({ entry: null, name: '' }))}
                    >
                      {t('tools_mkdir')}
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                  </>
                ) : null}
                <DropdownMenuCheckboxItem
                  checked={hidden}
                  disabled={preferences.disabled}
                  onCheckedChange={(v) => void preferences.save({ filesShowHidden: v })}
                >
                  {t('files_hidden')}
                </DropdownMenuCheckboxItem>
                <DropdownMenuCheckboxItem
                  checked={tree}
                  disabled={preferences.disabled}
                  onCheckedChange={(value) => void preferences.save({ filesShowTree: value })}
                >
                  {t('files_directory_tree')}
                </DropdownMenuCheckboxItem>
                {['size', 'modified', 'kind', 'permissions', 'owner'].map((key) => (
                  <DropdownMenuCheckboxItem
                    key={key}
                    checked={columns.includes(key)}
                    disabled={preferences.disabled}
                    onCheckedChange={(value) =>
                      void preferences.save({
                        filesColumns: fileColumns.filter((c) =>
                          c === key ? value : columns.includes(c),
                        ),
                      })
                    }
                  >
                    {label(key)}
                  </DropdownMenuCheckboxItem>
                ))}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
        {panelActions ? <div className="file-panel-actions">{panelActions}</div> : null}
      </div>
      <div className="file-browser-content" inert={!online}>
        {preferences.error ? <p role="alert">{t('error_storage_unavailable')}</p> : null}
        <div className="file-filter">
          <IconButton
            variant="quiet-toggle"
            size="icon-sm"
            aria-expanded={treeVisible}
            onClick={() => {
              if (width < 600) {
                void preferences.save({ filesShowTree: true });
                setNarrowTree((v) => !v);
              } else {
                void preferences.save({ filesShowTree: !tree });
              }
            }}
            label={t(treeVisible ? 'files_hide_tree' : 'files_show_tree')}
            tooltip={<>{t(treeVisible ? 'files_hide_tree' : 'files_show_tree')}</>}
          >
            {treeVisible ? <PanelLeftCloseIcon /> : <PanelLeftOpenIcon />}
          </IconButton>
          <Input
            variant="filter"
            className="w-45"
            aria-label={t('files_filter')}
            placeholder={t('files_filter')}
            value={filter}
            onChange={(e) => {
              setFilter(e.target.value);
              setSelection(new Set());
            }}
          />
          <span role="status">
            {loading
              ? t('tools_loading')
              : error
                ? error === 'restored_path_unavailable'
                  ? t('settings_path_unavailable')
                  : t(errorKey(error))
                : `${files.length} · ${chosen.length} ${t('files_selected')}`}
          </span>
        </div>
        <ResizablePanelGroup
          orientation="horizontal"
          className="file-content"
          onLayoutChanged={(_layout, meta) => {
            if (meta.isUserInteraction && treePanel.current)
              layout.treeWidth = treePanel.current.getSize().inPixels;
          }}
        >
          {treeVisible ? (
            <FileDirectoryPanel
              panelRef={treePanel}
              width={layout.treeWidth}
              minSize="100px"
              maxSize="40%"
              className="file-tree"
            >
              <ContextMenu
                open={online && contextOpen && menu?.source === 'tree'}
                onOpenChange={setContextOpen}
              >
                <ContextMenuTrigger render={<div className="min-h-full" />}>
                  <FileDirectoryTree
                    key={`${session.groupId}:${treeRevision}`}
                    session={session}
                    online={online}
                    currentPath={directory.path}
                    contextPath={
                      contextOpen && menu?.source === 'tree' ? menu.directory : undefined
                    }
                    onContextMenu={(entry) =>
                      setMenu(
                        fileMenuTarget(
                          'tree',
                          directory.path,
                          entry,
                          [],
                          workspace.runtimeId,
                          session.groupId,
                        ),
                      )
                    }
                    navigate={(path) => void load(path)}
                  />
                </ContextMenuTrigger>
                {menuContent}
              </ContextMenu>
            </FileDirectoryPanel>
          ) : null}
          {treeVisible ? <ResizableHandle /> : null}
          <ResizablePanel minSize="160px">
            <ContextMenu
              open={online && contextOpen && menu?.source !== 'tree'}
              onOpenChange={setContextOpen}
            >
              <ContextMenuTrigger
                render={<div className="file-table-wrap" />}
                onContextMenuCapture={() => {
                  setMenu(
                    fileMenuTarget(
                      'blank',
                      directory.path,
                      null,
                      [],
                      workspace.runtimeId,
                      session.groupId,
                    ),
                  );
                }}
              >
                <ResizableTableContainer
                  ref={scroll}
                  className="file-table-scroll"
                  onScroll={(e) => {
                    pref.scroll = e.currentTarget.scrollTop;
                    service.persist(session);
                  }}
                  onResizeEnd={(widths) => {
                    const columnWidths: [string, number][] = [];
                    for (const [key, width] of widths)
                      if (typeof key === 'string' && typeof width === 'number')
                        columnWidths.push([key, Math.max(40, Math.min(2000, Math.round(width)))]);
                    layout.widths = new Map(columnWidths);
                  }}
                >
                  <Virtualizer
                    layout={TableLayout}
                    layoutOptions={{ rowHeight: 28, headingHeight: 28 }}
                  >
                    <Table
                      aria-label={t('tools_files')}
                      className="file-table"
                      selectionMode="multiple"
                      selectionBehavior="replace"
                      selectedKeys={selection}
                      onSelectionChange={setSelection}
                      sortDescriptor={sort}
                      onSortChange={setSort}
                      disabledKeys={disabled ? new Set(files.map((e) => e.path)) : undefined}
                      onRowAction={(key) => {
                        const entry = files.find((e) => e.path === key);
                        if (entry) void openEntry(entry);
                      }}
                    >
                      <TableHeader>
                        {columns.map((column) => (
                          <Column
                            key={column}
                            id={column}
                            isRowHeader={column === 'name'}
                            allowsSorting={['name', 'size', 'modified'].includes(column)}
                            defaultWidth={
                              layout.widths.get(column) ??
                              (column === 'name'
                                ? '1fr'
                                : column === 'modified'
                                  ? 164
                                  : column === 'permissions'
                                    ? 112
                                    : column === 'owner'
                                      ? 128
                                      : 80)
                            }
                            minWidth={column === 'name' ? 160 : 80}
                          >
                            <span>
                              {label(column)}
                              {sort.column === column
                                ? sort.direction === 'ascending'
                                  ? ' ↑'
                                  : ' ↓'
                                : ''}
                            </span>
                            <ColumnResizer aria-label={label(column)} />
                          </Column>
                        ))}
                      </TableHeader>
                      <TableBody items={files} dependencies={[columns, names, t]}>
                        {(entry) => (
                          <Row
                            id={entry.path}
                            textValue={entry.name}
                            onContextMenu={() => {
                              setMenu(
                                fileMenuTarget(
                                  'list',
                                  directory.path,
                                  entry,
                                  chosen,
                                  workspace.runtimeId,
                                  session.groupId,
                                ),
                              );
                              setSelection((previous) =>
                                previous === 'all' || previous.has(entry.path)
                                  ? previous
                                  : new Set([entry.path]),
                              );
                            }}
                          >
                            {columns.map((column) => (
                              <Cell key={column}>{cell(entry, column)}</Cell>
                            ))}
                          </Row>
                        )}
                      </TableBody>
                    </Table>
                  </Virtualizer>
                </ResizableTableContainer>
                {online && files.length === 0 ? (
                  <div className="file-table-state" role={directoryError ? 'alert' : 'status'}>
                    <Empty className="my-auto">
                      <EmptyHeader>
                        <EmptyMedia variant="icon">
                          {loading ? (
                            <Spinner aria-label={t('tools_loading')} />
                          ) : directoryError ? (
                            <CircleAlertIcon aria-hidden="true" />
                          ) : filter ? (
                            <SearchIcon aria-hidden="true" />
                          ) : hiddenOnly ? (
                            <EyeOffIcon aria-hidden="true" />
                          ) : (
                            <FolderOpenIcon aria-hidden="true" />
                          )}
                        </EmptyMedia>
                        <EmptyTitle>{emptyTitle}</EmptyTitle>
                        {directoryError ? (
                          <EmptyDescription>{t(errorKey(directoryError))}</EmptyDescription>
                        ) : !loading && hiddenOnly ? (
                          <EmptyDescription>
                            {t('files_hidden_entries_description')}
                          </EmptyDescription>
                        ) : null}
                      </EmptyHeader>
                      {!loading && !directoryError && (filter || hiddenOnly) ? (
                        <EmptyContent>
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={filter ? false : preferences.disabled}
                            onClick={() => {
                              if (filter) setFilter('');
                              else void preferences.save({ filesShowHidden: true });
                            }}
                          >
                            {t(filter ? 'files_clear_filter' : 'files_hidden')}
                          </Button>
                        </EmptyContent>
                      ) : null}
                    </Empty>
                  </div>
                ) : null}
              </ContextMenuTrigger>
              {menuContent}
            </ContextMenu>
          </ResizablePanel>
        </ResizablePanelGroup>
        {operationsOpen ? (
          <FileOperationsDialog
            sessionId={session.id}
            initialId={operationId}
            close={() => setOperationsOpen(false)}
          />
        ) : null}
        {permissionEntries ? (
          <FilePermissionsDialog
            entries={permissionEntries}
            error={permissionError}
            close={() => setPermissionEntries(null)}
            apply={(value) =>
              startFileOperation(
                permissionEntries.map((entry) => entry.path),
                'permissions',
                value.recursive,
                value.setMask,
                value.clearMask,
              )
            }
          />
        ) : null}
        <FormDialog
          open={online && !!edit}
          onOpenChange={(open) => {
            if (!open) void closeNameDialog();
          }}
        >
          <DialogContent closeLabel={t('ws_close')}>
            <DialogHeader>
              <DialogTitle>
                {t(
                  edit?.entry
                    ? 'tools_rename'
                    : edit?.kind === 'file'
                      ? 'files_create_file'
                      : 'tools_mkdir',
                )}
              </DialogTitle>
            </DialogHeader>
            <DialogForm
              onSubmit={(event) => {
                event.preventDefault();
                void submitName();
              }}
              onKeyDownCapture={(event) => {
                if (
                  event.key === 'Enter' &&
                  (event.nativeEvent.isComposing || event.keyCode === 229)
                )
                  event.preventDefault();
              }}
            >
              <Field data-invalid={!!editError}>
                <FieldLabel htmlFor={editNameId}>{t('tools_filename')}</FieldLabel>
                <Input
                  id={editNameId}
                  value={edit?.name ?? ''}
                  disabled={editBusy}
                  aria-invalid={!!editError}
                  aria-describedby={editError ? `${editNameId}-error` : undefined}
                  onChange={(event) => {
                    setEditError(null);
                    setEdit((old) => old && { ...old, name: event.target.value });
                  }}
                />
                <FieldError id={`${editNameId}-error`}>
                  {editError
                    ? t(editError === 'invalid_input' ? 'files_name_invalid' : errorKey(editError))
                    : undefined}
                </FieldError>
              </Field>
              <DialogFooter className="mt-4">
                <Button
                  type="button"
                  variant="outline"
                  disabled={editBusy}
                  onClick={() => void closeNameDialog()}
                >
                  {t('ws_cancel')}
                </Button>
                <Button type="submit" disabled={disabled || editBusy || !edit?.name}>
                  {t('ws_confirm')}
                </Button>
              </DialogFooter>
            </DialogForm>
          </DialogContent>
        </FormDialog>
        <Dialog
          open={online && !!property}
          onOpenChange={(open) => {
            if (!open) setProperty(null);
          }}
        >
          <DialogContent closeLabel={t('ws_close')}>
            <DialogHeader>
              <DialogTitle>{t('files_properties')}</DialogTitle>
            </DialogHeader>
            {property ? (
              <dl className="file-properties">
                {[
                  property.path,
                  fileSize(property.size),
                  permissions(property),
                  ownership(property),
                ].map((value, i) => (
                  <div key={i}>{value}</div>
                ))}
              </dl>
            ) : null}
          </DialogContent>
        </Dialog>
      </div>
      {!online ? (
        <div
          className="file-offline-overlay"
          role="status"
          onContextMenu={(event) => event.preventDefault()}
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => event.preventDefault()}
        >
          <Empty className="p-0">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <FolderOpenIcon aria-hidden="true" />
              </EmptyMedia>
              <EmptyTitle>{t('files_available_after_connect')}</EmptyTitle>
            </EmptyHeader>
          </Empty>
        </div>
      ) : null}
    </div>
  );
}

export { useFileBrowser } from './model/use-file-browser';
