import { Switch } from '../../components/ui/switch';
import { Row } from '../settings/components/SettingsControls';
import { useDesktop } from '../../app/DesktopProvider';
import { useMcp } from './model';
import { McpFeedback } from './McpFeedback';
export function AgentTrayPreference() {
  const { t } = useDesktop(),
    model = useMcp();
  const settings = model.snapshot?.settings;
  return (
    <>
      <Row id="general-tray" label={t('mcp_tray')} description={t('mcp_tray_hint')}>
        <Switch
          id="general-tray"
          aria-describedby="general-tray-description"
          checked={settings?.showTray ?? true}
          disabled={!settings || model.busy}
          onCheckedChange={(showTray) => {
            if (settings)
              void model.issue({
                action: 'updateSettings',
                patch: { group: 'tray', showTray, baseShowTray: settings.showTray },
              });
          }}
        />
      </Row>
      <McpFeedback code={model.error} />
    </>
  );
}
