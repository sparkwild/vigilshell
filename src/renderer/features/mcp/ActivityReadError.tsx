import { Alert, AlertDescription } from '../../components/ui/alert';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
import { useDesktop } from '../../app/DesktopProvider';
export function ActivityReadError({ code }: { code: string | null }) {
  const { t } = useDesktop();
  return code ? (
    <Alert variant="destructive">
      <AlertDescription>{t(userErrorKey(code, 'agent_operation_failed'))}</AlertDescription>
    </Alert>
  ) : null;
}
