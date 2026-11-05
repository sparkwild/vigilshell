import { XIcon } from 'lucide-react';
import { IconButton } from '../../components/ui/icon-button';
import { useDesktop } from '../../app/DesktopProvider';

export function TerminalCloseButton({
  label,
  disabled,
  onClick,
  className,
  actionLabel,
}: {
  label: string;
  disabled?: boolean;
  onClick(): void;
  className?: string;
  actionLabel?: string;
}) {
  const { t } = useDesktop();
  return (
    <IconButton
      label={`${actionLabel ?? t('ws_close_terminal')} — ${label}`}
      tooltip={actionLabel ?? t('ws_close_terminal')}
      side="bottom"
      variant="tab-close"
      size="icon-tab"
      className={className}
      disabled={disabled}
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
    >
      <XIcon />
    </IconButton>
  );
}
