import type { ConnectionDetails, TrustView, TerminalSessionView } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace, errorCode } from '../connections/workspace-context';
import { useConfirmation } from '../../shared/ConfirmationProvider';
import { ConfirmationDialog } from '../../shared/ConfirmationDialog';
import { Button } from '../../components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../../components/ui/dialog';

export type CloseTransferChoice = 'cancel' | 'background' | 'cancelTransfers';
export function TransferCloseDialog({
  session,
  choose,
}: {
  session: TerminalSessionView | null;
  choose(choice: CloseTransferChoice): void;
}) {
  const { t } = useDesktop();
  return (
    <ConfirmationDialog
      open={!!session}
      title={t('tools_close_title')}
      description={t('tools_close_body')}
      tone="warning"
      cancelLabel={t('tools_keep_terminal')}
      onCancel={() => choose('cancel')}
      size="wide"
    >
      <Button variant="outline" onClick={() => choose('background')}>
        {t('tools_background_close')}
      </Button>
      <Button variant="destructive" onClick={() => choose('cancelTransfers')}>
        {t('tools_cancel_close')}
      </Button>
    </ConfirmationDialog>
  );
}
export function HostIdentityDialog({
  value,
  close,
  update,
}: {
  value: { connection: ConnectionDetails; trust: TrustView | null } | null;
  close(): void;
  update(value: { connection: ConnectionDetails; trust: TrustView | null }): void;
}) {
  const { t } = useDesktop(),
    { store } = useWorkspace(),
    confirm = useConfirmation();
  return (
    <Dialog
      open={!!value}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogContent closeLabel={t('ws_close')}>
        <DialogHeader>
          <DialogTitle>{t('ws_trust')}</DialogTitle>
        </DialogHeader>
        {value ? (
          <div className="flex flex-col gap-3">
            <strong>{value.connection.config.name}</strong>
            {value.trust ? (
              <>
                <span>{value.trust.algorithm}</span>
                <code className="break-all">{value.trust.fingerprint}</code>
                <Button
                  variant="outline"
                  onClick={() => void window.vigilshell.copyText(value.trust!.fingerprint)}
                >
                  {t('ws_copy')}
                </Button>
                <Button
                  variant="destructive"
                  onClick={async () => {
                    if (
                      !(await confirm({
                        tone: 'danger',
                        title: t('ws_forget_trust_title'),
                        description: t('ws_forget_trust_body'),
                        confirmLabel: t('ws_confirm'),
                        cancelLabel: t('ws_cancel'),
                      }))
                    )
                      return;
                    try {
                      await store.mutation('forgetTrust', {
                        id: value.connection.id,
                        version: value.connection.version,
                      });
                      const reply = await store.call('getConnection', {
                        id: value.connection.id,
                      });
                      if (reply.kind === 'connection')
                        update({
                          connection: reply.connection,
                          trust: reply.trust,
                        });
                      await store.refresh();
                    } catch (e) {
                      store.report(errorCode(e));
                    }
                  }}
                >
                  {t('ws_forget_trust')}
                </Button>
              </>
            ) : (
              <span>{t('ws_not_trusted')}</span>
            )}
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
