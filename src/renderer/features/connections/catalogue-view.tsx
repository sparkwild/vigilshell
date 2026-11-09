import { TerminalIcon } from 'lucide-react';
import { ConnectionTree } from './ConnectionTree';
import { useWorkspace } from './workspace-context';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '../../components/ui/empty';

export const ConnectionNavigation = ConnectionTree;

export function ConnectionOverview() {
  const { store, state } = useWorkspace(),
    { t } = useDesktop();
  if (!state.ready || state.error) return null;
  return (
    <Empty className="h-full">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <TerminalIcon />
        </EmptyMedia>
        <EmptyTitle>{t('ws_select_connection')}</EmptyTitle>
        <EmptyDescription>{t('ws_empty_hint')}</EmptyDescription>
      </EmptyHeader>
      <Button onClick={() => void store.newConnection()}>{t('ws_new_connection')}</Button>
    </Empty>
  );
}
