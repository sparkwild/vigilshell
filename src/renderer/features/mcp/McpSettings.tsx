import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';
import { SettingsFooterContent } from '../settings/components/SettingsFooter';
import { useState } from 'react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { FieldDescription } from '../../components/ui/field';
import { useMcp } from './model';
import { McpFeedback } from './McpFeedback';
import { McpServiceSettings } from './McpServiceSettings';
import { McpAccessSettings } from './McpAccessSettings';
export function McpSettings({ footer }: { footer: HTMLElement | null }) {
  const { t, snapshot: desktop } = useDesktop(),
    model = useMcp();
  const [message, setMessage] = useState('');
  const runtimeUnavailable = desktop.runtime?.status === 'unavailable';
  const error = runtimeUnavailable ? null : (model.error ?? model.snapshot?.error ?? null);
  const feedback = useNotificationSurface(error ? 'mcp:' : null);
  if (!model.snapshot)
    return (
      <>
        <FieldDescription>{t('mcp_loading')}</FieldDescription>
        <McpFeedback code={error} />
        <Button
          variant="outline"
          disabled={runtimeUnavailable}
          onClick={() => void model.refresh()}
        >
          {t('mcp_retry')}
        </Button>
      </>
    );
  return (
    <>
      <McpServiceSettings model={model} message={setMessage} />
      <McpAccessSettings model={model} />
      <div ref={feedback}>
        <McpFeedback code={error} />
      </div>
      <SettingsFooterContent target={footer}>
        <span className="settings-save-status" role="status">
          {model.busy ? t('agent_auth_wait') : message}
        </span>
      </SettingsFooterContent>
    </>
  );
}
