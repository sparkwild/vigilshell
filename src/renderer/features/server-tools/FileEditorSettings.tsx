import { useState } from 'react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Row } from '../settings/components/SettingsControls';
import { Alert, AlertDescription } from '../../components/ui/alert';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
export function FileEditorSettings() {
  const { t, snapshot } = useDesktop();
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  const application = snapshot.preferences?.fileEditorApplication;
  return (
    <>
      <Row
        id="file-editor-application"
        label={t('file_editor_default')}
        description={application || t('file_editor_unset')}
      >
        <Button
          id="file-editor-application"
          variant="outline"
          size="sm"
          disabled={busy || !window.vigilshell.fileEditors}
          onClick={async () => {
            setBusy(true);
            setError(null);
            try {
              const result = await window.vigilshell.fileEditors.request({
                action: 'chooseEditor',
              });
              if (!result.ok) setError(result.error ?? 'files_unsupported');
            } catch {
              setError('runtime_unavailable');
            } finally {
              setBusy(false);
            }
          }}
        >
          {t('file_editor_choose')}
        </Button>
      </Row>
      {error ? (
        <Alert>
          <AlertDescription>{t(userErrorKey(error))}</AlertDescription>
        </Alert>
      ) : null}
    </>
  );
}
