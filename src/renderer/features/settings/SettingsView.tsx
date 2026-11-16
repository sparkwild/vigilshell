import { ApplicationSettings } from './ApplicationSettings';
import { UpdateBadge } from './UpdateIndicators';
import { UiStateFeedback } from './UiStateFeedback';
import { MonitorSettings } from './components/MonitorSettings';
import { FileEditorSettings } from '../server-tools/FileEditorSettings';
import { SettingsFooter } from './components/SettingsFooter';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AgentActivitySettings } from '../mcp/AgentActivitySettings';
import { AgentTrayPreference } from '../mcp/AgentTrayPreference';
import type { LeaveGuard } from '../mcp/SettingsParts';
import type { SettingsCategory } from './settings-navigation';
import { McpSettings } from '../mcp/McpSettings';
import { AgentLogo } from '../mcp/McpClientPicker';
import { NotificationSettings } from './NotificationSettings';
import { Tooltip, TooltipTrigger, TooltipContent } from '../../components/ui/tooltip';

import type { ErrorCode } from '@contracts';

import { ArrowLeftIcon, CheckIcon } from 'lucide-react';

import { errorMessage } from '../../../shared/i18n/messages';

import { fileColumns, scrollbackOptions } from '../../../shared/preferences';

import { Button } from '../../components/ui/button';

import { Alert, AlertDescription } from '../../components/ui/alert';

import { Card, CardContent } from '../../components/ui/card';

import { FieldDescription, FieldGroup, FieldSet, FieldLegend } from '../../components/ui/field';

import { Input } from '../../components/ui/input';

import { ToggleGroup, ToggleGroupItem } from '../../components/ui/toggle-group';

import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../components/ui/select';

import { Switch } from '../../components/ui/switch';

import {
  SidebarProvider,
  SidebarContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
} from '../../components/ui/sidebar';

import { Spinner } from '../../components/ui/spinner';

import { Row, ChoiceSelect, NumberPreference } from './components/SettingsControls';

import { useSettingsController } from './model/use-settings-controller';

import { categories, columnLabels } from './settings-navigation';
import { SettingsRuntimeFeedback } from '../../app/RuntimeFeedback';

export function SettingsView({
  onBack,
  initialCategory = 'general',
  navigationVersion = 0,
  registerExitGuard,
}: {
  onBack(): void;
  initialCategory?: SettingsCategory;
  navigationVersion?: number;
  registerExitGuard?: (guard: LeaveGuard | null) => void;
}) {
  const model = useSettingsController(initialCategory);
  const leave = useRef<LeaveGuard | null>(null);
  const [footerSlot, setFooterSlot] = useState<HTMLDivElement | null>(null);
  const register = useCallback((guard: LeaveGuard | null) => {
    leave.current = guard;
  }, []);
  const navigate = useCallback(async (action: () => void) => {
    if (!leave.current || (await leave.current())) action();
  }, []);
  useEffect(() => {
    registerExitGuard?.(async () => !leave.current || (await leave.current()));
    return () => registerExitGuard?.(null);
  }, [registerExitGuard]);
  const changeCategory = model.setCategory;
  useEffect(() => {
    void navigate(() => {
      changeCategory(initialCategory);
    });
  }, [navigationVersion, initialCategory, navigate, changeCategory]);
  const {
    store,
    snapshot,
    t,
    p,
    save,
    category,
    setCategory,
    fontUnavailable,
    fontItems,
    heading,
    busy,
    pending,
    title,
    numeric,
    options,
    chooseDirectory,
    failure,
    retry,
  } = model;

  return (
    <SidebarProvider className="settings-shell">
      <aside className="settings-sidebar" aria-label={t('settings')}>
        <div className="native-titlebar" aria-hidden="true" />
        <SidebarHeader className="settings-sidebar-header">
          <Button variant="sidebar" className="justify-start" onClick={() => void navigate(onBack)}>
            <ArrowLeftIcon data-icon="inline-start" />
            {t('back')}
          </Button>
        </SidebarHeader>
        <SidebarContent className="px-3">
          <SidebarMenu>
            {categories.map((c) => (
              <SidebarMenuItem key={c.id}>
                <SidebarMenuButton
                  isActive={category === c.id}
                  aria-current={category === c.id ? 'page' : undefined}
                  onClick={() =>
                    void navigate(() => {
                      setCategory(c.id);
                    })
                  }
                >
                  {'logo' in c ? <AgentLogo client={c.logo} /> : <c.icon />}
                  <span>{t(c.label)}</span>
                  {c.id === 'about' ? <UpdateBadge /> : null}
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarContent>
      </aside>
      <main className="settings-main">
        <div className="native-titlebar" aria-hidden="true" />
        <div className="settings-scroll">
          <div className="settings-content">
            <header>
              <h1 ref={heading} tabIndex={-1} className="page-title outline-none">
                {title}
              </h1>
            </header>
            {category === 'about' ? (
              <ApplicationSettings />
            ) : category === 'mcp' ? (
              <McpSettings footer={footerSlot} />
            ) : category === 'agentActivity' ? (
              <AgentActivitySettings footer={footerSlot} register={register} />
            ) : category === 'notifications' ? (
              <NotificationSettings
                api={window.vigilshell.notifications}
                preferences={snapshot.preferences}
                locale={snapshot.appearance.locale}
                disabled={busy}
                onUpdate={save}
              />
            ) : (
              <Card appearance="settings">
                <CardContent className="p-0">
                  <FieldGroup className="gap-0">
                    {category === 'general' ? (
                      <>
                        <AgentTrayPreference />
                        <Row
                          id="theme-select"
                          label={t('theme')}
                          description={t('theme_description')}
                        >
                          <Select
                            items={options([
                              ['system', 'system'],
                              ['light', 'light'],
                              ['dark', 'dark'],
                            ])}
                            value={p.themeMode}
                            disabled={busy}
                            onValueChange={(v) => {
                              if (v === 'system' || v === 'light' || v === 'dark')
                                void save({ themeMode: v });
                            }}
                          >
                            <SelectTrigger
                              id="theme-select"
                              aria-describedby="theme-select-description"
                              size="sm"
                            >
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectGroup>
                                {options([
                                  ['system', 'system'],
                                  ['light', 'light'],
                                  ['dark', 'dark'],
                                ]).map((o) => (
                                  <SelectItem key={o.value} value={o.value}>
                                    {o.label}
                                  </SelectItem>
                                ))}
                              </SelectGroup>
                            </SelectContent>
                          </Select>
                        </Row>
                        <Row
                          id="language-select"
                          label={t('language')}
                          description={t('language_description')}
                        >
                          <Select
                            items={options([
                              ['system', 'auto_detect'],
                              ['zh-CN', 'chinese'],
                              ['en', 'english'],
                            ])}
                            value={p.localeMode}
                            disabled={busy}
                            onValueChange={(v) => {
                              if (v === 'system' || v === 'zh-CN' || v === 'en')
                                void save({ localeMode: v });
                            }}
                          >
                            <SelectTrigger
                              id="language-select"
                              aria-describedby="language-select-description"
                              size="sm"
                            >
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectGroup>
                                {options([
                                  ['system', 'auto_detect'],
                                  ['zh-CN', 'chinese'],
                                  ['en', 'english'],
                                ]).map((o) => (
                                  <SelectItem key={o.value} value={o.value}>
                                    {o.label}
                                  </SelectItem>
                                ))}
                              </SelectGroup>
                            </SelectContent>
                          </Select>
                        </Row>
                        <Row
                          id="sidebar-preview"
                          label={t('settings_sidebar_preview')}
                          description={t('settings_sidebar_preview_description')}
                        >
                          <Switch
                            id="sidebar-preview"
                            aria-describedby="sidebar-preview-description"
                            checked={p.sidebarHoverPreview}
                            disabled={busy}
                            onCheckedChange={(value) => void save({ sidebarHoverPreview: value })}
                          />
                        </Row>
                      </>
                    ) : null}
                    {category === 'terminal' ? (
                      <>
                        <Row
                          id="terminal-theme"
                          label={t('settings_terminal_theme')}
                          description={t('settings_terminal_theme_description')}
                        >
                          <ChoiceSelect
                            id="terminal-theme"
                            value={p.terminalThemeMode}
                            disabled={busy}
                            items={options([
                              ['inherit', 'settings_follow_app'],
                              ['light', 'light'],
                              ['dark', 'dark'],
                            ])}
                            onChange={(value) => void save({ terminalThemeMode: value })}
                          />
                        </Row>
                        <Row
                          id="terminal-font"
                          label={t('settings_terminal_font')}
                          description={t(
                            fontUnavailable
                              ? 'settings_font_unavailable_description'
                              : 'settings_terminal_font_description',
                          )}
                        >
                          <ChoiceSelect
                            id="terminal-font"
                            value={p.terminalFontFamily}
                            disabled={busy}
                            items={fontItems}
                            onChange={(value) => void save({ terminalFontFamily: value })}
                          />
                        </Row>
                        <NumberPreference
                          id="terminal-size"
                          label={t('settings_font_size')}
                          description={t('settings_font_size_description')}
                          value={p.terminalFontSize}
                          min={10}
                          max={24}
                          disabled={busy}
                          onCommit={(v) => void save({ terminalFontSize: v })}
                        />
                        <NumberPreference
                          id="terminal-line"
                          label={t('settings_line_height')}
                          description={t('settings_line_height_description')}
                          value={p.terminalLineHeight / 10}
                          min={1}
                          max={2}
                          step={0.1}
                          disabled={busy}
                          onCommit={(v) =>
                            void save({
                              terminalLineHeight: Math.round(v * 10),
                            })
                          }
                        />
                        <Row
                          id="terminal-scrollback"
                          label={t('settings_scrollback')}
                          description={t('settings_scrollback_description')}
                        >
                          <ChoiceSelect
                            id="terminal-scrollback"
                            value={String(p.terminalScrollback)}
                            items={numeric(scrollbackOptions)}
                            disabled={busy}
                            onChange={(v) => {
                              const selected = scrollbackOptions.find((n) => String(n) === v);
                              if (selected !== undefined)
                                void save({ terminalScrollback: selected });
                            }}
                          />
                        </Row>
                        <Row
                          id="terminal-cursor"
                          label={t('settings_cursor')}
                          description={t('settings_cursor_description')}
                        >
                          <ChoiceSelect
                            id="terminal-cursor"
                            value={p.terminalCursorStyle}
                            items={options([
                              ['block', 'settings_cursor_block'],
                              ['bar', 'settings_cursor_bar'],
                              ['underline', 'settings_cursor_underline'],
                            ])}
                            disabled={busy}
                            onChange={(v) => void save({ terminalCursorStyle: v })}
                          />
                        </Row>
                        <Row
                          id="terminal-blink"
                          label={t('settings_cursor_blink')}
                          description={t('settings_cursor_blink_description')}
                        >
                          <Switch
                            id="terminal-blink"
                            aria-describedby="terminal-blink-description"
                            checked={p.terminalCursorBlink}
                            disabled={busy}
                            onCheckedChange={(v) => void save({ terminalCursorBlink: v })}
                          />
                        </Row>
                        <Row
                          id="terminal-copy"
                          label={t('settings_copy_on_select')}
                          description={t('settings_copy_on_select_description')}
                        >
                          <Switch
                            id="terminal-copy"
                            aria-describedby="terminal-copy-description"
                            checked={p.terminalCopyOnSelect}
                            disabled={busy}
                            onCheckedChange={(value) => void save({ terminalCopyOnSelect: value })}
                          />
                        </Row>
                      </>
                    ) : null}
                    {category === 'connection' ? (
                      <Row
                        id="connection-open"
                        label={t('settings_duplicate_open')}
                        description={t('settings_duplicate_open_description')}
                      >
                        <ChoiceSelect
                          id="connection-open"
                          value={p.connectionOpenBehavior}
                          items={options([
                            ['ask', 'settings_ask'],
                            ['reuse', 'ws_reuse_terminal'],
                            ['new', 'ws_new_terminal'],
                          ])}
                          disabled={busy}
                          onChange={(v) => void save({ connectionOpenBehavior: v })}
                        />
                      </Row>
                    ) : null}
                    {category === 'files' ? (
                      <>
                        <FileEditorSettings />
                        <Row
                          id="files-auto-open"
                          label={t('settings_files_auto_open')}
                          description={t('settings_files_auto_open_description')}
                        >
                          <Switch
                            id="files-auto-open"
                            aria-describedby="files-auto-open-description"
                            checked={p.filesAutoOpen}
                            disabled={busy}
                            onCheckedChange={(value) => void save({ filesAutoOpen: value })}
                          />
                        </Row>
                        <Row
                          id="remote-directory-mode"
                          label={t('settings_remote_directory_mode')}
                          description={t('settings_remote_directory_mode_description')}
                        >
                          <ChoiceSelect
                            id="remote-directory-mode"
                            value={p.remoteDirectoryMode}
                            disabled={busy}
                            items={options([
                              ['initial', 'settings_directory_initial'],
                              ['last', 'settings_directory_last'],
                            ])}
                            onChange={(value) => void save({ remoteDirectoryMode: value })}
                          />
                        </Row>
                        <Row
                          id="files-hidden"
                          label={t('files_hidden')}
                          description={t('settings_files_hidden_description')}
                        >
                          <Switch
                            id="files-hidden"
                            aria-describedby="files-hidden-description"
                            checked={p.filesShowHidden}
                            disabled={busy}
                            onCheckedChange={(v) => void save({ filesShowHidden: v })}
                          />
                        </Row>
                        <Row
                          id="files-tree"
                          label={t('files_directory_tree')}
                          description={t('settings_files_tree_description')}
                        >
                          <Switch
                            id="files-tree"
                            aria-describedby="files-tree-description"
                            checked={p.filesShowTree}
                            disabled={busy}
                            onCheckedChange={(v) => void save({ filesShowTree: v })}
                          />
                        </Row>
                        <FieldSet appearance="settings" aria-describedby="file-columns-description">
                          <FieldLegend id="file-columns-label" className="mb-0 w-full">
                            {t('settings_file_columns')}
                          </FieldLegend>
                          <FieldDescription id="file-columns-description" className="w-full">
                            {t('settings_file_columns_description')}
                          </FieldDescription>
                          <ToggleGroup
                            multiple
                            variant="outline"
                            size="sm"
                            className="max-w-full flex-wrap"
                            aria-labelledby="file-columns-label"
                            aria-describedby="file-columns-description"
                            value={p.filesColumns}
                            disabled={busy}
                            onValueChange={(values) =>
                              void save({
                                filesColumns: fileColumns.filter(
                                  (column) => column === 'name' || values.includes(column),
                                ),
                              })
                            }
                          >
                            {fileColumns.map((column) => (
                              <ToggleGroupItem
                                key={column}
                                value={column}
                                disabled={busy || column === 'name'}
                              >
                                <CheckIcon
                                  data-icon="inline-start"
                                  className={
                                    p.filesColumns.includes(column) ? undefined : 'invisible'
                                  }
                                />
                                {t(columnLabels[column])}
                              </ToggleGroupItem>
                            ))}
                          </ToggleGroup>
                        </FieldSet>
                        <Row
                          id="download-mode"
                          label={t('settings_download_location')}
                          description={t(
                            p.downloadMode === 'directory'
                              ? 'settings_download_fixed_description'
                              : 'settings_download_ask_description',
                          )}
                        >
                          <ChoiceSelect
                            id="download-mode"
                            value={p.downloadMode}
                            items={options([
                              ['ask', 'settings_download_ask'],
                              ['directory', 'settings_download_fixed'],
                            ])}
                            disabled={busy}
                            onChange={(v) => {
                              if (v === 'directory' && !p.downloadDirectory) void chooseDirectory();
                              else void save({ downloadMode: v });
                            }}
                          />
                        </Row>
                        {p.downloadMode === 'directory' ? (
                          <Row
                            id="download-directory"
                            label={t('settings_download_folder')}
                            description={t('settings_download_folder_description')}
                          >
                            <div className="flex w-full min-w-0 max-w-md flex-wrap items-center gap-2">
                              <Tooltip>
                                <TooltipTrigger
                                  render={
                                    <Input
                                      id="download-directory"
                                      aria-describedby="download-directory-description"
                                      className="min-w-32 flex-1"
                                      value={p.downloadDirectory}
                                      readOnly
                                    />
                                  }
                                />
                                <TooltipContent>{p.downloadDirectory}</TooltipContent>
                              </Tooltip>
                              <Button
                                variant="outline"
                                size="sm"
                                disabled={busy}
                                onClick={() => void chooseDirectory()}
                              >
                                {t('settings_change_directory')}
                              </Button>
                            </div>
                          </Row>
                        ) : null}
                      </>
                    ) : null}
                    {category === 'monitor' ? (
                      <MonitorSettings preferences={p} busy={busy} save={save} />
                    ) : null}
                  </FieldGroup>
                </CardContent>
              </Card>
            )}
          </div>
        </div>
        <SettingsFooter
          slotRef={setFooterSlot}
          feedback={
            <>
              <SettingsRuntimeFeedback />
              {category === 'files' ? <UiStateFeedback scope="pickers" /> : null}
              {failure && snapshot.runtime.status === 'ready' ? (
                <Alert variant="destructive" className="w-fit max-w-full">
                  <AlertDescription>
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                      <span>
                        {t(
                          failure === 'storage_unavailable'
                            ? 'settings_save_failed'
                            : (errorMessage[failure as ErrorCode] ?? 'error_host_unavailable'),
                        )}
                      </span>
                      {retry ? (
                        <Button size="xs" variant="outline" onClick={() => void retry()}>
                          {t('retry')}
                        </Button>
                      ) : null}
                    </div>
                  </AlertDescription>
                </Alert>
              ) : null}
              {snapshot.save.phase === 'unknown' ? (
                <div className="flex gap-2">
                  <Button size="sm" variant="outline" onClick={() => void store.reconcile()}>
                    {t('settings_reconcile')}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => void store.restart()}>
                    {t('settings_restart')}
                  </Button>
                </div>
              ) : null}
            </>
          }
        >
          <div className="settings-save-status" role="status" aria-live="polite">
            {pending ? (
              <>
                <Spinner aria-hidden="true" />
                {t(snapshot.save.phase === 'confirming' ? 'settings_confirming' : 'saving')}
              </>
            ) : null}
          </div>
        </SettingsFooter>
      </main>
    </SidebarProvider>
  );
}

export { useSettingsController } from './model/use-settings-controller';

export { categories } from './settings-navigation';
