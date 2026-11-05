import type { RemoteEntry } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import {
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '../../components/ui/context-menu';
import type { FileMenuTarget } from './file-menu-target';

export function FileContextMenu({
  target,
  disabled,
  refresh,
  open,
  upload,
  download,
  archive,
  create,
  rename,
  remove,
  properties,
  permissions,
}: {
  target: FileMenuTarget | null;
  disabled: boolean;
  refresh(): void;
  open(entry: RemoteEntry, alternative?: boolean): void;
  upload(path: string): void;
  download(paths: string[]): void;
  archive(paths: string[]): void;
  create(path: string, kind: 'file' | 'directory'): void;
  rename(entry: RemoteEntry): void;
  remove(paths: string[]): void;
  properties(path: string): void;
  permissions(paths: string[]): void;
}) {
  const { t } = useDesktop();
  if (!target) return null;
  const entries = target.entries,
    single = entries.length === 1 ? entries[0] : undefined,
    paths =
      target.source === 'tree' && !entries.length ? [target.directory] : entries.map((e) => e.path),
    transferable =
      paths.length > 0 &&
      paths.length <= 100 &&
      entries.every((e) => ['file', 'directory'].includes(e.kind));
  return (
    <ContextMenuContent>
      <ContextMenuGroup>
        {target.source === 'list' ? (
          <ContextMenuItem
            disabled={
              disabled || !single || !['file', 'directory', 'symlink'].includes(single.kind)
            }
            onClick={() => single && open(single)}
          >
            {t('files_open')}
          </ContextMenuItem>
        ) : null}
        {single?.kind === 'file' ? (
          <ContextMenuSub>
            <ContextMenuSubTrigger disabled={disabled}>
              {t('files_open_with')}
            </ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuGroup>
                <ContextMenuItem disabled={disabled} onClick={() => open(single)}>
                  {t('files_default_editor')}
                </ContextMenuItem>
                <ContextMenuItem disabled={disabled} onClick={() => open(single, true)}>
                  {t('files_other_editor')}
                </ContextMenuItem>
              </ContextMenuGroup>
            </ContextMenuSubContent>
          </ContextMenuSub>
        ) : null}
        <ContextMenuItem disabled={disabled} onClick={refresh}>
          {t('tools_refresh')}
        </ContextMenuItem>
      </ContextMenuGroup>
      <ContextMenuSeparator />
      <ContextMenuGroup>
        {target.source === 'list' ? (
          <ContextMenuItem
            onClick={() =>
              void navigator.clipboard.writeText(entries.map((e) => e.name).join('\n'))
            }
          >
            {t('files_copy_name')}
          </ContextMenuItem>
        ) : null}
        <ContextMenuItem
          onClick={() =>
            void navigator.clipboard.writeText(paths.length ? paths.join('\n') : target.directory)
          }
        >
          {t('files_copy_path')}
        </ContextMenuItem>
      </ContextMenuGroup>
      <ContextMenuSeparator />
      <ContextMenuGroup>
        {target.source !== 'blank' ? (
          <ContextMenuItem disabled={disabled || !transferable} onClick={() => download(paths)}>
            {t('tools_download')}
          </ContextMenuItem>
        ) : null}
        <ContextMenuItem disabled={disabled} onClick={() => upload(target.directory)}>
          {t('tools_upload')}
        </ContextMenuItem>
        {target.source !== 'blank' ? (
          <ContextMenuItem disabled={disabled || !transferable} onClick={() => archive(paths)}>
            {t('files_archive_download')}
          </ContextMenuItem>
        ) : null}
      </ContextMenuGroup>
      <ContextMenuSeparator />
      <ContextMenuGroup>
        <ContextMenuSub>
          <ContextMenuSubTrigger disabled={disabled}>{t('ws_new')}</ContextMenuSubTrigger>
          <ContextMenuSubContent>
            <ContextMenuGroup>
              <ContextMenuItem
                disabled={disabled}
                onClick={() => create(target.directory, 'directory')}
              >
                {t('files_new_directory')}
              </ContextMenuItem>
              <ContextMenuItem disabled={disabled} onClick={() => create(target.directory, 'file')}>
                {t('files_new_file')}
              </ContextMenuItem>
            </ContextMenuGroup>
          </ContextMenuSubContent>
        </ContextMenuSub>
        {target.source !== 'blank' ? (
          <>
            <ContextMenuItem
              disabled={disabled || !single || single.path === '/'}
              onClick={() => single && rename(single)}
            >
              {t('tools_rename')}
            </ContextMenuItem>
            <ContextMenuItem
              variant="destructive"
              disabled={disabled || !paths.length || paths.length > 100 || paths.includes('/')}
              onClick={() => remove(paths)}
            >
              {t('ws_delete')}
            </ContextMenuItem>
          </>
        ) : null}
      </ContextMenuGroup>
      <ContextMenuSeparator />
      <ContextMenuGroup>
        {target.source !== 'blank' ? (
          <ContextMenuItem
            disabled={
              disabled ||
              !paths.length ||
              paths.length > 100 ||
              entries.some((e) => !['file', 'directory'].includes(e.kind))
            }
            onClick={() => permissions(paths)}
          >
            {t('files_permissions_title')}
          </ContextMenuItem>
        ) : null}
        <ContextMenuItem
          disabled={disabled || (target.source === 'list' && !single)}
          onClick={() => properties(single?.path ?? target.directory)}
        >
          {t('files_properties')}
        </ContextMenuItem>
      </ContextMenuGroup>
    </ContextMenuContent>
  );
}
