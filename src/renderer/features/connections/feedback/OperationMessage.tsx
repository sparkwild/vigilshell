import { connectionFailureTitle } from '../connection-diagnostics';

import { translate } from '../../../../shared/i18n/messages';

import { Button } from '../../../components/ui/button';

import { useConnections } from '../ConnectionProvider';

import {
  operationRunning,
  operationCanCancel,
  operationCanReconcile,
  type Operation,
} from '../operations';

import { text, errorText } from '../text';

export function OperationMessage({
  operation,
  onView,
  onNavigate,
}: {
  operation: Operation;
  onView?: () => void;
  onNavigate?: () => void;
}) {
  const state = useConnections();
  const t = (key: Parameters<typeof text>[1]) => text(state.snapshot.appearance.locale, key);
  const connectionFailure =
    operation.phase === 'failed' &&
    (operation.job?.kind === 'connect' ||
      ['ws_connect', 'ws_connect_new'].includes(operation.action));
  const target = connectionFailure
    ? state.sessions.find(
        (session) => session.id === operation.job?.sessionId && session.phase === 'failed',
      )
    : undefined;
  const view = target
    ? () => {
        window.dispatchEvent(
          new CustomEvent('vigilshell:terminal-diagnostics', { detail: target.id }),
        );
        onNavigate?.();
      }
    : onView;
  return (
    <div className="flex flex-col gap-2">
      <span>
        {connectionFailure
          ? translate(
              connectionFailureTitle(target?.error ?? operation.error?.code ?? null),
              state.snapshot.appearance.locale,
            )
          : operation.error
            ? operation.error.code === 'result_unknown'
              ? t('unknown')
              : operation.error.code === 'conflict'
                ? t('conflict')
                : errorText(state.snapshot.appearance.locale, operation.error.code)
            : t(operation.phase === 'cancelling' ? 'cancellingOperation' : 'pending')}
      </span>
      {operation.error?.code === 'credential_denied' ||
      (operationRunning(operation) &&
        ['ws_connect', 'ws_connect_new', 'ws_test', 'ws_save', 'ws_save_connect'].includes(
          operation.action,
        )) ? (
        <span>{translate('ws_keychain_help', state.snapshot.appearance.locale)}</span>
      ) : null}
      {operation.cancelError ? (
        <span>
          {operation.cancelError.code === 'too_late'
            ? t('cancelTooLate')
            : `${t('cancelOperation')} · ${errorText(state.snapshot.appearance.locale, operation.cancelError.code)}`}
        </span>
      ) : null}
      <div className="flex flex-wrap gap-2 empty:hidden">
        {view ? (
          <Button size="xs" variant="outline" onClick={view}>
            {target
              ? translate('connection_details', state.snapshot.appearance.locale)
              : t('viewOperation')}
          </Button>
        ) : null}
        {operationRunning(operation) && operationCanCancel(operation) ? (
          <Button
            size="xs"
            variant="outline"
            disabled={operation.phase === 'cancelling'}
            onClick={() => void state.operations.cancel(operation.id)}
          >
            {t('cancelOperation')}
          </Button>
        ) : null}
        {operation.phase === 'unknown' && operationCanReconcile(operation) ? (
          <Button
            size="xs"
            variant="outline"
            onClick={() =>
              void state.operations
                .reconcile(operation.id)
                .then(() => state.refresh())
                .catch(() => undefined)
            }
          >
            {t('reconcile')}
          </Button>
        ) : null}
        {operation.phase === 'failed' && operation.owner === 'navigation:connections' ? (
          <Button
            size="xs"
            variant="outline"
            disabled={state.operations.isBusy(operation.owner)}
            onClick={() => void state.store.retryConnections()}
          >
            {translate('ws_retry', state.snapshot.appearance.locale)}
          </Button>
        ) : null}
        {operation.phase === 'failed' ? (
          <Button size="xs" variant="ghost" onClick={() => state.operations.dismiss(operation.id)}>
            {connectionFailure
              ? translate('connection_dismiss_notice', state.snapshot.appearance.locale)
              : t('close')}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
