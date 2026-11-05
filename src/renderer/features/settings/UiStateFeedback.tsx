import { useUiStateErrors, retryUiState } from '../../shared/platform/ui-state-store';
import { useDesktop } from '../../app/DesktopProvider';
import { Alert, AlertDescription } from '../../components/ui/alert';
import { Button } from '../../components/ui/button';
export function UiStateFeedback({ scope }: { scope: string }) {
  const scopes = useUiStateErrors();
  const { t } = useDesktop();
  if (!scopes.includes(scope)) return null;
  return (
    <Alert>
      <AlertDescription>
        {t('settings_state_unsaved')}
        <Button size="xs" variant="link" onClick={() => void retryUiState(scope)}>
          {t('retry')}
        </Button>
      </AlertDescription>
    </Alert>
  );
}
