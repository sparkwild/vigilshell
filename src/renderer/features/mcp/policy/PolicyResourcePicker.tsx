import { useCallback, useRef, useState } from 'react';
import { RefreshCwIcon, XIcon } from 'lucide-react';
import type { AgentFilesPage, ConnectionSummary } from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../../../components/ui/dialog';
import { PolicyResourceTree } from './PolicyResourceTree';
import { Button } from '../../../components/ui/button';
import { IconButton } from '../../../components/ui/icon-button';
import { Input } from '../../../components/ui/input';
import { PathInput } from '../../../components/ui/path-input';
import { TruncatedText } from '../../../components/ui/tooltip';
import { Spinner } from '../../../components/ui/spinner';
import { Alert, AlertDescription } from '../../../components/ui/alert';
import { McpDetails } from '../McpDetails';
import { validTarget } from '../TargetList';
import { usePolicyBrowser } from './use-policy-browser';

export type PickedResource = { path: string; kind: 'file' | 'directory' };
export function PolicyResourcePicker({
  connection,
  selected,
  close,
  apply,
}: {
  connection: ConnectionSummary;
  selected: PickedResource[];
  close(): void;
  apply(values: PickedResource[]): Promise<boolean>;
}) {
  const { t } = useDesktop(),
    browser = usePolicyBrowser(connection);
  const [path, setPath] = useState('/');
  const [target, setTarget] = useState({ path: '/', version: 0 });
  const [refresh, setRefresh] = useState(0),
    [treeBusy, setTreeBusy] = useState(false);
  const [selection, setSelection] = useState(selected),
    [search, setSearch] = useState('');
  const [address, setAddress] = useState('/'),
    [editing, setEditing] = useState(false);
  const [inputError, setInputError] = useState(false),
    [saving, setSaving] = useState(false),
    [saveError, setSaveError] = useState(false);
  const baseline = useRef(selected);
  const flight = useRef(false);
  const { read } = browser;
  const load = (value: string) => {
    if (!validTarget(value)) {
      setInputError(true);
      return;
    }
    setInputError(false);
    setSearch('');
    setEditing(false);
    setPath(value);
    setAddress(value);
    setTarget((old) => ({ path: value, version: old.version + 1 }));
  };
  const readDirectory = useCallback(
    async (value: string, after: string | null): Promise<AgentFilesPage | null> => {
      const result = await read({ kind: 'directory', path: value, after });
      return result?.kind === 'directory' ? result.page : null;
    },
    [read],
  );
  const toggle = (value: PickedResource, checked: boolean) =>
    setSelection((old) =>
      checked
        ? old.some((v) => v.path === value.path) || old.length >= 128
          ? old
          : [...old, value]
        : old.filter((v) => v.path !== value.path),
    );
  const busy = treeBusy || saving;
  const changed =
    selection.length !== baseline.current.length ||
    selection.some((v) => !baseline.current.some((s) => s.path === v.path && s.kind === v.kind));
  const submit = async () => {
    if (flight.current || !changed || busy || selection.length > 128) return;
    flight.current = true;
    setSaving(true);
    setSaveError(false);
    try {
      if (await apply(selection)) close();
      else setSaveError(true);
    } catch {
      setSaveError(true);
    } finally {
      flight.current = false;
      setSaving(false);
    }
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !flight.current) close();
      }}
    >
      <DialogContent
        size="wide"
        closeLabel={t('close')}
        className="flex h-[min(40rem,calc(100dvh-4rem))] flex-col overflow-hidden"
      >
        <DialogHeader className="shrink-0 pr-6">
          <DialogTitle>{t('policy_choose_server_paths')}</DialogTitle>
          <DialogDescription>
            {connection.name} · {t('policy_picker_purpose')}
          </DialogDescription>
        </DialogHeader>
        {inputError || saveError ? (
          <Alert>
            <AlertDescription>
              {t(inputError ? 'policy_directory_path_invalid' : 'policy_picker_save_error')}
            </AlertDescription>
          </Alert>
        ) : null}
        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <div className="min-w-0 flex-1">
            <PathInput
              path={path}
              value={address}
              editing={editing}
              disabled={busy}
              label={t('policy_scope_path')}
              editLabel={t('policy_picker_edit_path')}
              onValueChange={setAddress}
              onEditingChange={setEditing}
              onNavigate={(value) => void load(value)}
            />
          </div>
          <IconButton
            variant="outline"
            label={t('policy_picker_retry')}
            disabled={busy}
            onClick={() => setRefresh((v) => v + 1)}
          >
            <RefreshCwIcon />
          </IconButton>
        </div>
        <Input
          aria-label={t('search')}
          placeholder={t('policy_tree_filter')}
          value={search}
          disabled={saving}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div className="min-h-0 flex-1 overflow-auto overscroll-contain">
          <PolicyResourceTree
            read={readDirectory}
            selection={selection}
            toggle={toggle}
            search={search}
            saving={saving}
            target={target}
            refresh={refresh}
            onNavigate={setPath}
            onBusy={setTreeBusy}
          />
        </div>
        <div className="shrink-0">
          <McpDetails label={t('policy_picker_selected', { count: selection.length })}>
            <div className="max-h-28 overflow-auto">
              {selection.map((item) => (
                <div key={item.path} className="flex min-w-0 items-center gap-2">
                  <Button
                    variant="link"
                    size="sm"
                    className="min-w-0 flex-1 justify-start"
                    disabled={busy}
                    onClick={() => load(item.path)}
                  >
                    <TruncatedText>{item.path}</TruncatedText>
                  </Button>
                  <IconButton
                    variant="ghost"
                    size="icon-xs"
                    label={t('agent_remove_target', { target: item.path })}
                    disabled={busy}
                    onClick={() => toggle(item, false)}
                  >
                    <XIcon />
                  </IconButton>
                </div>
              ))}
            </div>
          </McpDetails>
        </div>
        <DialogFooter className="shrink-0">
          <Button variant="ghost" disabled={saving} onClick={close}>
            {t('ws_cancel')}
          </Button>
          <Button
            disabled={busy || !changed || selection.length > 128}
            onClick={() => void submit()}
          >
            {saving ? <Spinner /> : null}
            {t('policy_picker_save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
