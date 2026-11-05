import { IconButton } from '../../components/ui/icon-button';
import { CircleHelpIcon } from 'lucide-react';
import { useDesktop } from '../../app/DesktopProvider';

export function DefaultValueHint({ label, children }: { label: string; children: string }) {
  const { t } = useDesktop();
  return (
    <IconButton
      type="button"
      variant="hint"
      size="icon-hint"
      label={`${label} · ${t('ws_default_hint')}`}
      tooltip={<>{children}</>}
    >
      <CircleHelpIcon aria-hidden="true" />
    </IconButton>
  );
}
