import { CircleFadingArrowUpIcon } from 'lucide-react';
import { useApplicationState } from '../../app/ApplicationProvider';
import { useDesktop } from '../../app/DesktopProvider';
import { Badge } from '../../components/ui/badge';
import { IconButton } from '../../components/ui/icon-button';
import { Spinner } from '../../components/ui/spinner';
import { useOpenUpdate } from './UpdateController';
export function UpdateBadge() {
  const state = useApplicationState();
  const { t } = useDesktop();
  return state.release ? (
    <Badge variant="update" className="ml-auto">
      {t('updates_available')}
    </Badge>
  ) : null;
}
export function UpdateButton() {
  const state = useApplicationState();
  const show = useOpenUpdate();
  const { t } = useDesktop();
  if (!state.release) return null;
  const busy = ['downloading', 'preparing', 'installing'].includes(state.phase);
  const label =
    state.phase === 'ready'
      ? t('updates_ready')
      : state.phase === 'downloading'
        ? `${t('updates_downloading')} ${Math.floor(state.progress?.percent ?? 0)}%`
        : ['downloadError', 'installError'].includes(state.phase)
          ? t('updates_view')
          : t('updates_available_version', { version: state.release.version });
  return (
    <IconButton
      label={label}
      variant="update"
      size="icon"
      onClick={show}
      data-testid="open-update"
      aria-busy={busy}
    >
      {busy ? <Spinner aria-hidden="true" /> : <CircleFadingArrowUpIcon aria-hidden="true" />}
    </IconButton>
  );
}
