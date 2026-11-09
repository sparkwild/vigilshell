import { useRef } from 'react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { ConfirmationDialog } from '../../shared/ConfirmationDialog';
import type { ConnectionOpenChoice, ConnectionOpenView } from './connection-open-controller';

export function ConnectionOpenDialog({
  view,
  choose,
  focusReusedTerminal,
}: {
  view: ConnectionOpenView | null;
  choose(choice: ConnectionOpenChoice): void;
  focusReusedTerminal?(): void;
}) {
  const { t } = useDesktop();
  const lastChoice = useRef<ConnectionOpenChoice>('cancel');
  const select = (choice: ConnectionOpenChoice) => {
    lastChoice.current = choice;
    choose(choice);
  };
  const reason = view?.newBlocked;
  return (
    <ConfirmationDialog
      open={view !== null}
      tone="neutral"
      title={t('ws_existing_terminal_title')}
      description={
        view
          ? t(view.count === 0 ? 'ws_existing_terminals_ended' : 'ws_existing_terminal_body', {
              name: view.name,
              count: view.count,
            })
          : ''
      }
      detail={
        reason ? (
          <p className="text-sm text-muted-foreground" role="status">
            {t(
              reason === 'connection_limit'
                ? 'ws_terminal_connection_limit'
                : reason === 'total_limit'
                  ? 'ws_terminal_total_limit'
                  : 'ws_terminal_creation_busy',
            )}
          </p>
        ) : undefined
      }
      cancelLabel={t('ws_cancel')}
      cancelPlacement="start"
      onCancel={() => select('cancel')}
      onClosed={() => {
        if (lastChoice.current === 'reuse') focusReusedTerminal?.();
        lastChoice.current = 'cancel';
      }}
    >
      <Button
        type="button"
        variant="outline"
        disabled={view?.count === 0}
        onClick={() => select('reuse')}
      >
        {t('ws_reuse_terminal')}
      </Button>
      <Button type="button" disabled={!!reason} onClick={() => select('new')}>
        {t('ws_new_terminal')}
      </Button>
    </ConfirmationDialog>
  );
}
