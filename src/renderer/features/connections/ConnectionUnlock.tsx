import { Alert, AlertTitle, AlertDescription } from '../../components/ui/alert';
import { useConnections } from './ConnectionProvider';
import { errorText } from './text';
import { Button } from '../../components/ui/button';
import { translate } from '../../../shared/i18n/messages';

export function ConnectionUnlock() {
  const state = useConnections();
  const locale = state.snapshot.appearance.locale;
  if (state.available || (!state.error && !state.loading)) return null;
  const access = state.connectionAccess;
  const waiting = access?.phase === 'locked' || access?.phase === 'unlocking';
  return (
    <Alert variant="sidebar" aria-busy={waiting || state.loading}>
      <AlertTitle>{translate('ws_connections_locked', locale)}</AlertTitle>
      <AlertDescription>
        {state.error && !waiting
          ? errorText(locale, state.error)
          : translate('ws_connections_unlocking', locale)}
      </AlertDescription>
      {access?.phase === 'blocked' ? (
        <Button size="sm" variant="outline" onClick={() => void state.store.retryConnections()}>
          {translate('retry', locale)}
        </Button>
      ) : null}
    </Alert>
  );
}
