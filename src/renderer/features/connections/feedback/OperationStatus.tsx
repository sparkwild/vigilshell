import { useNotificationFeedback } from '../../notifications/NotificationFeedbackProvider';
import { FeedbackItem } from '../../notifications/FeatureFeedback';
import { translate } from '../../../../shared/i18n/messages';

import { useState, type ReactNode } from 'react';

import { AlertCircleIcon, XIcon } from 'lucide-react';

import { cn } from 'cn';

import { Button } from '../../../components/ui/button';

import { useDelayedBusy } from '../../../components/ui/use-delayed-busy';

import { Spinner } from '../../../components/ui/spinner';

import { Popover, PopoverTrigger, PopoverContent } from '../../../components/ui/popover';

import { useConnections } from '../ConnectionProvider';

import { operationRunning, operationBusy, type Operation } from '../operations';

import { text, errorText } from '../text';

import { useOperationAnchor } from './use-operation-anchor';

import { OperationMessage } from './OperationMessage';

export function OperationIndicator({
  owner,
  relatedOwner,
  icon,
  action,
  className,
  externalBusy = false,
}: {
  owner: string;
  relatedOwner?: string;
  icon: ReactNode;
  action?: Operation['action'];
  className?: string;
  externalBusy?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const state = useConnections();
  const externalVisible = useDelayedBusy(externalBusy, owner);
  const ref = useOperationAnchor<HTMLDivElement>(owner);
  const feedback = useNotificationFeedback();
  const connectionIds = owner.startsWith('connection:')
    ? [owner.slice(11)]
    : owner.startsWith('group:')
      ? (state.connections ?? []).filter((c) => c.groupId === owner.slice(6)).map((c) => c.id)
      : [];
  const notices = feedback.filter(
    (item) =>
      item.relevant &&
      item.target.kind === 'connection' &&
      connectionIds.includes(item.target.connectionId),
  );
  const orphan = state.operationState?.operations.find(
    (op) =>
      op.visible &&
      (op.resources?.includes(owner) ||
        (op.job?.sessionId &&
          state.sessions.some(
            (session) =>
              session.id === op.job?.sessionId && connectionIds.includes(session.connectionId),
          ))),
  );

  const relatedRef = useOperationAnchor<HTMLDivElement>(relatedOwner ?? null);
  const direct = state.operations.forOwner(owner);
  const primary = orphan && operationBusy(orphan) ? orphan : (direct ?? orphan);
  const secondary = relatedOwner ? state.operations.forOwner(relatedOwner) : undefined;
  const operation =
    primary && operationBusy(primary)
      ? primary
      : secondary && operationBusy(secondary)
        ? secondary
        : (primary ?? secondary);
  const shown =
    (operation?.visible && (!action || operation.action === action)) || notices.length > 0;
  const label = operation
    ? `${operation.label} · ${translate(operation.action, state.snapshot.appearance.locale)}`
    : translate('feedback_attention', state.snapshot.appearance.locale);
  return (
    <span
      className={cn('inline-flex size-4 shrink-0 items-center justify-center', className)}
      data-operation-owner={owner}
    >
      {shown ? (
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger
            aria-label={label}
            render={<Button type="button" size="icon-xs" variant="titlebar" />}
          >
            {operation?.error || notices.length ? (
              <AlertCircleIcon
                className={
                  operation?.phase === 'failed' ? 'text-destructive' : 'text-status-warning'
                }
              />
            ) : (
              <Spinner aria-hidden="true" />
            )}
          </PopoverTrigger>
          <PopoverContent>
            <div
              ref={(node) => {
                ref.current = node;
                relatedRef.current = node;
              }}
              className="flex max-h-80 flex-col gap-2 overflow-auto"
            >
              {operation ? (
                <OperationMessage operation={operation} onNavigate={() => setOpen(false)} />
              ) : null}
              {notices.map((item) => (
                <FeedbackItem key={item.id} item={item} />
              ))}
            </div>
          </PopoverContent>
        </Popover>
      ) : externalVisible ? (
        <Spinner aria-label={text(state.snapshot.appearance.locale, 'connecting')} />
      ) : (
        icon
      )}
    </span>
  );
}

/** 按钮内部只替换图标；取消和详细反馈由相邻状态位承载，避免嵌套按钮。 */
export function OperationButtonIcon({
  owner,
  action,
  children,
}: {
  owner: string;
  action: Operation['action'];
  children: ReactNode;
}) {
  const { operations } = useConnections();
  const operation = operations.forOwner(owner);
  return operation?.visible && operationRunning(operation) && operation.action === action ? (
    <Spinner aria-hidden="true" />
  ) : (
    children
  );
}

export function OperationStatus({
  owner,
  message,
  details,
  relatedOwner,
  dismissible = false,
}: {
  owner: string;
  relatedOwner?: string;
  dismissible?: boolean;
  message?: string | null;
  details?: ReactNode;
}) {
  const [dismissed, setDismissed] = useState<string | null>(null);
  const state = useConnections();
  const ref = useOperationAnchor(owner);
  const relatedRef = useOperationAnchor(relatedOwner ?? null);
  const primary = state.operations.forOwner(owner);
  const secondary = relatedOwner ? state.operations.forOwner(relatedOwner) : undefined;
  const operation =
    primary && operationBusy(primary)
      ? primary
      : secondary && operationBusy(secondary)
        ? secondary
        : (primary ?? secondary);
  const visible = operation?.visible;
  const label = visible
    ? operation.error
      ? errorText(state.snapshot.appearance.locale, operation.error.code)
      : `${translate(operation.action, state.snapshot.appearance.locale)} · ${text(state.snapshot.appearance.locale, operation.phase === 'cancelling' ? 'cancellingOperation' : 'pending')}`
    : message;
  const notice = JSON.stringify([
    owner,
    operation?.id,
    operation?.phase,
    operation?.error?.code,
    message,
  ]);
  return (
    <span
      ref={(node) => {
        ref.current = node;
        relatedRef.current = node;
      }}
      data-operation-status={owner}
      className="flex h-6 min-w-0 items-center gap-2 text-xs text-muted-foreground"
      role="status"
      aria-live="polite"
    >
      {label && (!dismissible || dismissed !== notice) ? (
        <>
          <Popover>
            <PopoverTrigger
              render={
                <Button type="button" variant="link" size="xs" className="max-w-full min-w-0" />
              }
            >
              <span className="truncate">{label}</span>
            </PopoverTrigger>
            <PopoverContent>
              {visible ? <OperationMessage operation={operation} /> : <span>{message}</span>}
              {details}
            </PopoverContent>
          </Popover>
          {dismissible ? (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="shrink-0"
              aria-label={text(state.snapshot.appearance.locale, 'close')}
              onClick={() => setDismissed(notice)}
            >
              <XIcon aria-hidden="true" />
            </Button>
          ) : null}
        </>
      ) : null}
    </span>
  );
}
