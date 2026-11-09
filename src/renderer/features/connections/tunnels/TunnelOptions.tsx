import { useEffect, useState } from 'react';

import { PencilIcon, PlusIcon, Trash2Icon, RotateCwIcon } from 'lucide-react';

import type { TunnelRule } from '@contracts';

import { useDesktop } from '../../../app/DesktopProvider';

import { Button } from '../../../components/ui/button';

import { Checkbox } from '../../../components/ui/checkbox';

import { FieldDescription, FieldGroup } from '../../../components/ui/field';

import { useWorkspace, errorCode, errorKey } from '../workspace-context';

import { TunnelEditor } from './TunnelEditor';

export function TunnelOptions({
  value,
  onChange,
  disabled,
  connectionId,
  onChildClose,
}: {
  value: TunnelRule[];
  onChange(value: TunnelRule[]): void;
  disabled: boolean;
  connectionId: string | null;
  onChildClose(close: (() => Promise<void>) | null): void;
}) {
  const { store, state } = useWorkspace(),
    { t } = useDesktop();
  const [editing, setEditing] = useState<TunnelRule | null>(null),
    [error, setError] = useState<string | null>(null);
  const statuses = connectionId ? (state.tunnelStates.get(connectionId) ?? []) : [];
  useEffect(() => {
    if (connectionId) void store.loadTunnels(connectionId).catch(() => undefined);
  }, [connectionId, store]);
  async function retry(id: string) {
    if (!connectionId) return;
    try {
      await store.run({
        owner: 'tunnel:' + id,
        action: 'ws_retry',
        label: value.find((r) => r.id === id)?.name ?? t('ws_tunnels'),
        execute: () => store.mutation('retryTunnel', { connectionId, ruleId: id }),
      });
      setError(null);
    } catch (e) {
      setError(errorCode(e));
    }
  }
  return (
    <FieldGroup density="tight">
      <FieldDescription>{t('ws_tunnel_lifecycle')}</FieldDescription>
      <div className="connection-option-list">
        {value.map((rule) => {
          const status = statuses.find((s) => s.ruleId === rule.id);
          return (
            <div key={rule.id} className="connection-option-row">
              <Checkbox
                aria-label={t('ws_enabled') + ' · ' + rule.name}
                checked={rule.enabled}
                disabled={disabled}
                onCheckedChange={(enabled) =>
                  onChange(value.map((r) => (r.id === rule.id ? { ...r, enabled } : r)))
                }
              />
              <div className="min-w-0 flex-1">
                <p className="truncate">{rule.name}</p>
                <p className="text-xs text-muted-foreground break-all">
                  {t(
                    rule.kind === 'local'
                      ? 'ws_tunnel_local'
                      : rule.kind === 'remote'
                        ? 'ws_tunnel_remote'
                        : 'ws_tunnel_dynamic',
                  )}{' '}
                  · {rule.bindHost}:{rule.listenPort}
                  {rule.targetHost ? ` → ${rule.targetHost}:${rule.targetPort}` : ''}
                </p>
                {status ? (
                  <p className="text-xs text-muted-foreground">
                    {t(
                      status.phase === 'running'
                        ? 'ws_tunnel_running'
                        : status.phase === 'failed'
                          ? 'ws_failed'
                          : 'ws_tunnel_stopped',
                    )}
                    {status.error ? ' · ' + t(errorKey(status.error)) : ''}
                  </p>
                ) : null}
              </div>
              {status?.error ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t('ws_retry') + ' · ' + rule.name}
                  onClick={() => void retry(rule.id)}
                  disabled={disabled || store.feedback.isBusy('tunnel:' + rule.id)}
                >
                  <RotateCwIcon />
                </Button>
              ) : null}
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                disabled={disabled}
                aria-label={t('ws_edit_tunnel') + ' · ' + rule.name}
                onClick={() => setEditing(rule)}
              >
                <PencilIcon />
              </Button>
              <Button
                type="button"
                variant="destructive"
                size="icon-xs"
                disabled={disabled}
                aria-label={t('ws_delete') + ' · ' + rule.name}
                onClick={() => onChange(value.filter((r) => r.id !== rule.id))}
              >
                <Trash2Icon />
              </Button>
            </div>
          );
        })}
      </div>
      <Button
        type="button"
        variant="outline"
        className="self-start"
        disabled={disabled || value.length >= 32}
        onClick={() =>
          setEditing({
            id: crypto.randomUUID(),
            name: '',
            kind: 'local',
            bindHost: '127.0.0.1',
            listenPort: 0,
            targetHost: '127.0.0.1',
            targetPort: 0,
            enabled: true,
          })
        }
      >
        <PlusIcon />
        {t('ws_new_tunnel')}
      </Button>
      <FieldDescription>{t('ws_reconnect_settings_hint')}</FieldDescription>
      {error ? <p role="alert">{t(errorKey(error))}</p> : null}
      {editing ? (
        <TunnelEditor
          key={editing.id}
          initial={editing}
          onChildClose={onChildClose}
          onClose={() => setEditing(null)}
          onSaved={(rule) => {
            onChange(
              value.some((r) => r.id === rule.id)
                ? value.map((r) => (r.id === rule.id ? rule : r))
                : [...value, rule],
            );
            setEditing(null);
          }}
        />
      ) : null}
    </FieldGroup>
  );
}
