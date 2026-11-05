import { CheckIcon } from 'lucide-react';
import type { WorkbenchTabLabel } from './workbench-tab-model';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { PopoverContent, PopoverTitle } from '../../components/ui/popover';
import { errorKey } from '../connections/workspace-context';
import { TerminalCloseButton } from './TerminalCloseButton';

export function TerminalSessionsPopover({
  labels,
  activeId,
  busy,
  errors,
  isClosing,
  select,
  close,
  closeAll,
  finalFocus,
}: {
  labels: WorkbenchTabLabel[];
  activeId: string;
  busy: boolean;
  errors: Record<string, string>;
  isClosing(id: string): boolean;
  select(id: string): void;
  close(id: string): void;
  closeAll(): void;
  finalFocus(): HTMLElement | boolean | null;
}) {
  const { t } = useDesktop();
  return (
    <PopoverContent
      data-terminal-session-list=""
      appearance="sessions"
      align="end"
      finalFocus={finalFocus}
    >
      <div className="terminal-session-heading">
        <PopoverTitle appearance="caption">{t('ws_all_terminals')}</PopoverTitle>
      </div>
      <div className="terminal-session-list">
        {labels.map(({ id, tab, label, phase, description }) => (
          <div className="terminal-session-row" key={id}>
            <div className="terminal-session-row-actions">
              <Button
                variant="ghost"
                size="list"
                onClick={() => select(id)}
                aria-label={`${label} — ${t(phase)}`}
                aria-current={id === activeId ? 'true' : undefined}
              >
                <span className="terminal-session-check">{id === activeId && <CheckIcon />}</span>
                <span className="terminal-session-description">
                  <span>{label}</span>
                  <span>{description}</span>
                </span>
                <span className="terminal-session-phase">{t(phase)}</span>
              </Button>
              <TerminalCloseButton
                label={label}
                actionLabel={tab.kind === 'agent' ? t('agent_view_hide') : undefined}
                disabled={busy || isClosing(id)}
                onClick={() => close(id)}
              />
            </div>
            {errors[id] && (
              <div className="terminal-session-error" role="alert">
                {t(errorKey(errors[id]!))}
              </div>
            )}
          </div>
        ))}
      </div>
      <div className="terminal-session-footer">
        <Button
          variant="destructive"
          size="compact"
          className="w-full"
          disabled={busy || !labels.length}
          onClick={closeAll}
        >
          {t('ws_close_all_terminals')}
        </Button>
      </div>
    </PopoverContent>
  );
}
