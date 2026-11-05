import { useConfirmation } from '../../../shared/ConfirmationProvider';

import { useEffect, useState } from 'react';

import { PencilIcon, PlusIcon, Trash2Icon } from 'lucide-react';

import type { ProxyView } from '@contracts';

import { useDesktop } from '../../../app/DesktopProvider';

import { Button } from '../../../components/ui/button';

import { FieldDescription, FieldGroup } from '../../../components/ui/field';

import { useWorkspace, errorCode, errorKey } from '../workspace-context';

import { WorkspaceFailure } from '../workspace-store';

import { readPages } from '../model/read-pages';

import { OptionSelect } from '../components/OptionSelect';

import { ProxyEditor } from './ProxyEditor';

export function ProxyOptions({
  value,
  onChange,
  disabled,
  credentialBlocked,
  onChildClose,
}: {
  value: string | null;
  onChange(value: string | null): void;
  disabled: boolean;
  credentialBlocked: boolean;
  onChildClose(close: (() => Promise<void>) | null): void;
}) {
  const { store, state } = useWorkspace(),
    { t } = useDesktop(),
    confirm = useConfirmation();
  const [proxies, setProxies] = useState<ProxyView[]>([]),
    [editing, setEditing] = useState<ProxyView | 'new' | null>(null),
    [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    void readPages<ProxyView>(async (offset, version) => {
      const r = await store.call('listProxies', { offset, version });
      if (r.kind !== 'proxies') throw new WorkspaceFailure('protocol_mismatch');
      return { items: r.proxies, version: r.version, next: r.next };
    })
      .then((proxies) => {
        if (active) {
          setProxies(proxies);
          setError(null);
        }
      })
      .catch((e) => {
        if (active) setError(t(errorKey(errorCode(e))));
      });
    return () => {
      active = false;
    };
  }, [store, state.version, state.runtimeId, t]);
  async function remove(proxy: ProxyView) {
    try {
      const references = await readPages<string>(async (offset, version) => {
        const r = await store.call('proxyReferences', {
          id: proxy.id,
          offset,
          version,
        });
        if (r.kind !== 'proxyReferences') throw new WorkspaceFailure('protocol_mismatch');
        return { items: r.connectionNames, version: r.version, next: r.next };
      });
      if (references.length) {
        setError(t('ws_proxy_references') + ' ' + references.join(', '));
        return;
      }
      if (
        !(await confirm({
          tone: 'danger',
          title: t('ws_delete_proxy_title'),
          description: t('ws_delete_proxy_body'),
          confirmLabel: t('ws_confirm'),
          cancelLabel: t('ws_cancel'),
        }))
      )
        return;
      await store.run({
        owner: 'proxy:' + proxy.id,
        action: 'ws_delete_proxy',
        label: proxy.config.name,
        execute: () =>
          store.mutation('deleteProxy', {
            id: proxy.id,
            version: proxy.version,
          }),
      });
      setProxies((p) => p.filter((v) => v.id !== proxy.id));
      if (value === proxy.id) onChange(null);
    } catch (e) {
      setError(t(errorKey(errorCode(e))));
    }
  }
  return (
    <FieldGroup density="tight">
      <OptionSelect
        id="connection-proxy"
        label={t('ws_proxy')}
        value={value ?? 'none'}
        disabled={disabled}
        onChange={(v) => onChange(v === 'none' ? null : v)}
        options={[
          { value: 'none', label: t('ws_no_proxy') },
          ...proxies.map((p) => ({ value: p.id, label: p.config.name })),
        ]}
      />
      <div className="connection-option-list">
        {proxies.map((p) => (
          <div key={p.id} className="connection-option-row">
            <div className="min-w-0 flex-1">
              <p className="truncate">{p.config.name}</p>
              <p className="text-xs text-muted-foreground truncate">
                {p.config.kind.toUpperCase()} · {p.config.host}:{p.config.port}
              </p>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              disabled={disabled}
              aria-label={t('ws_edit_proxy') + ' · ' + p.config.name}
              onClick={() => setEditing(p)}
            >
              <PencilIcon />
            </Button>
            <Button
              type="button"
              variant="destructive"
              size="icon-xs"
              disabled={disabled}
              aria-label={t('ws_delete_proxy') + ' · ' + p.config.name}
              onClick={() => void remove(p)}
            >
              <Trash2Icon />
            </Button>
          </div>
        ))}
      </div>
      <Button
        type="button"
        variant="outline"
        className="self-start"
        disabled={disabled}
        onClick={() => setEditing('new')}
      >
        <PlusIcon />
        {t('ws_new_proxy')}
      </Button>
      <FieldDescription>{t('ws_proxy_saved_independently')}</FieldDescription>
      {error ? (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : null}
      {editing ? (
        <ProxyEditor
          key={editing === 'new' ? 'new' : editing.id}
          proxy={editing === 'new' ? null : editing}
          credentialBlocked={credentialBlocked}
          onChildClose={onChildClose}
          onClose={() => setEditing(null)}
          onSaved={(p) => {
            setProxies((old) => [...old.filter((v) => v.id !== p.id), p]);
            onChange(p.id);
            setEditing(null);
          }}
        />
      ) : null}
    </FieldGroup>
  );
}
