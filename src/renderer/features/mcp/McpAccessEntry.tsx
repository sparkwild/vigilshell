import { CircleSlashIcon, Trash2Icon, SquarePenIcon } from 'lucide-react';
import type { AccessEntry } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { useConfirmation } from '../../shared/ConfirmationProvider';
import { AgentLogo } from './McpClientPicker';
import type { useMcp } from './model';
export function McpAccessEntry({
  entry,
  model,
  configure,
  highlighted,
}: {
  entry: AccessEntry;
  model: ReturnType<typeof useMcp>;
  configure(): void;
  highlighted?: boolean;
}) {
  const { t } = useDesktop();
  const confirm = useConfirmation();
  const observation = model
    .snapshot!.observations.filter((o) => o.entryId === entry.id)
    .reduce<number>((last, o) => Math.max(last, o.lastAt), 0);
  const revoke = async () => {
    const accepted = await confirm({
      title: t('agent_revoke_title', { name: entry.name }),
      description: t('agent_revoke_description'),
      confirmLabel: t('mcp_revoke'),
      cancelLabel: t('ws_cancel'),
      tone: 'danger',
    });
    if (accepted) await model.issue({ action: 'revokeAccess', id: entry.id });
  };
  const remove = async () => {
    const accepted = await confirm({
      title: t('agent_delete_access_title'),
      description: t('agent_delete_access_description'),
      confirmLabel: t('agent_delete_access'),
      cancelLabel: t('ws_cancel'),
      tone: 'danger',
    });
    if (accepted)
      await model.issue({ action: 'deleteAccess', id: entry.id, generation: entry.generation });
  };
  return (
    <div className="mcp-access-row" data-highlighted={highlighted || undefined}>
      <AgentLogo client={entry.client} />
      <div className="mcp-access-info">
        <span>{entry.name}</span>
        <p>
          {!entry.enabled
            ? t('agent_revoked')
            : observation
              ? t('agent_last_used', { time: new Date(observation).toLocaleString() })
              : t('agent_never_used')}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          size="sm"
          variant="ghost-contrast"
          disabled={model.busy || !entry.enabled}
          aria-label={t('agent_edit_named', { name: entry.name })}
          onClick={configure}
        >
          <SquarePenIcon data-icon="inline-start" aria-hidden="true" />
          {t('agent_configure_connection')}
        </Button>
        <Button
          size="sm"
          variant="destructive"
          disabled={model.busy}
          aria-label={t(entry.enabled ? 'agent_revoke_named' : 'agent_delete_named', {
            name: entry.name,
          })}
          onClick={() => void (entry.enabled ? revoke() : remove())}
        >
          {entry.enabled ? (
            <CircleSlashIcon data-icon="inline-start" aria-hidden="true" />
          ) : (
            <Trash2Icon data-icon="inline-start" aria-hidden="true" />
          )}
          {t(entry.enabled ? 'mcp_revoke' : 'agent_delete_access')}
        </Button>
      </div>
    </div>
  );
}
