import { CircleHelpIcon } from 'lucide-react';
import { useDesktop } from '../../app/DesktopProvider';
import { IconButton } from '../../components/ui/icon-button';

export function CredentialStorageNotice() {
  const { t } = useDesktop();
  return (
    <IconButton
      type="button"
      variant="hint"
      size="icon-hint"
      className="align-middle"
      label={t('ws_keychain_help_title')}
      tooltip={
        <div className="flex flex-col gap-2">
          <p>{t('ws_credential_storage_notice')}</p>
          <p>{t('ws_keychain_help')}</p>
        </div>
      }
    >
      <CircleHelpIcon aria-hidden="true" />
    </IconButton>
  );
}
