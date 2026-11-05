import { useState } from 'react';
import type { AgentClient } from '@contracts';
import { McpClientPicker } from './McpClientPicker';
import { useDesktop } from '../../app/DesktopProvider';
import { Badge } from '../../components/ui/badge';
import { FieldDescription } from '../../components/ui/field';
import { Empty, EmptyHeader, EmptyTitle, EmptyDescription } from '../../components/ui/empty';
import { SettingsGroup } from './SettingsParts';
import { McpAccessEntry } from './McpAccessEntry';
import { McpConnectionSetup } from './McpConnectionSetup';
import type { useMcp } from './model';

export function McpAccessSettings({ model }: { model: ReturnType<typeof useMcp> }) {
  const { t } = useDesktop();
  const snapshot = model.snapshot!;
  const [setup, setSetup] = useState<{ id: string | null; client?: AgentClient } | null>(null);
  const [highlighted, setHighlighted] = useState<string | null>(null);
  const atLimit = snapshot.entries.length >= 32;
  return (
    <section className="settings-section" aria-label={t('agent_client_access')}>
      <div className="mcp-access-header">
        <div className="mcp-access-heading">
          <h2 className="section-title">{t('agent_client_access')}</h2>
          <Badge variant="secondary">{snapshot.entries.length} / 32</Badge>
        </div>
        <McpClientPicker
          model={model}
          disabled={model.busy || atLimit}
          onSelect={(client) => setSetup({ id: null, client })}
        />
      </div>
      <SettingsGroup>
        {snapshot.entries.length ? (
          snapshot.entries.map((entry) => (
            <McpAccessEntry
              key={entry.id}
              entry={entry}
              model={model}
              highlighted={highlighted === entry.id}
              configure={() => setSetup({ id: entry.id })}
            />
          ))
        ) : (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{t('agent_access_empty')}</EmptyTitle>
              <EmptyDescription>{t('agent_access_empty_hint')}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </SettingsGroup>
      {atLimit ? <FieldDescription>{t('agent_connection_limit')}</FieldDescription> : null}
      {setup ? (
        <McpConnectionSetup
          entryId={setup.id}
          client={setup.client ?? 'generic'}
          model={model}
          onCreated={setHighlighted}
          close={() => setSetup(null)}
        />
      ) : null}
    </section>
  );
}
