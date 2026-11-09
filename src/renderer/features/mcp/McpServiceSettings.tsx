import { useState } from 'react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Switch } from '../../components/ui/switch';
import { Row } from '../settings/components/SettingsControls';
import { SettingsGroup } from './SettingsParts';
import type { useMcp } from './model';
export function McpServiceSettings({
  model,
  message,
}: {
  model: ReturnType<typeof useMcp>;
  message(value: string): void;
}) {
  const { t } = useDesktop();
  const [portOpen, setPortOpen] = useState(false),
    [port, setPort] = useState(51602);
  const snapshot = model.snapshot!,
    settings = snapshot.settings;
  const endpoint = snapshot.endpoint || 'http://127.0.0.1:51602/mcp';
  const setMessage = message;
  const service = (enabled: boolean, newPort = settings.port) =>
    model.issue({
      action: 'updateSettings',
      patch: {
        group: 'service',
        enabled,
        port: newPort,
        baseEnabled: settings.enabled,
        basePort: settings.port,
      },
    });
  return (
    <section className="settings-section">
      <h2 className="section-title">{t('agent_local_service')}</h2>
      <SettingsGroup>
        <Row id="mcp-service" label={t('mcp_enabled')} description={t('agent_service_lifecycle')}>
          <Switch
            id="mcp-service"
            checked={settings.enabled}
            disabled={model.busy}
            onCheckedChange={(v) => void service(v)}
          />
        </Row>
        <Row id="mcp-address" label={t('mcp_endpoint')} description={endpoint}>
          <Button
            variant="ghost"
            size="sm"
            disabled={model.busy}
            onClick={() => {
              setPort(settings.port || 51602);
              setPortOpen(!portOpen);
            }}
          >
            {t('agent_change_port')}
          </Button>
        </Row>
        {portOpen ? (
          <Row id="mcp-port" label={t('mcp_port')} description={t('mcp_port_hint')}>
            <div className="flex items-center gap-2">
              <Input
                id="mcp-port"
                type="number"
                min={0}
                max={65535}
                step={1}
                value={port}
                disabled={model.busy}
                onChange={(e) => setPort(Number(e.target.value))}
                onKeyDown={(e) => {
                  if (e.key === 'Escape') setPort(settings.port);
                }}
              />
              <Button
                size="sm"
                variant="outline"
                disabled={
                  model.busy ||
                  !Number.isInteger(port) ||
                  port < 0 ||
                  port > 65535 ||
                  port === settings.port
                }
                onClick={() =>
                  void service(settings.enabled, port).then((ok) => {
                    if (ok) {
                      setPortOpen(false);
                      setMessage(t('agent_port_changed'));
                    }
                  })
                }
              >
                {t('agent_save_settings')}
              </Button>
            </div>
          </Row>
        ) : null}
      </SettingsGroup>
    </section>
  );
}
