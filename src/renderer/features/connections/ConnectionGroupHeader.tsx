import { IconButton } from '../../components/ui/icon-button';
import { OperationIndicator } from './OperationFeedback';
import { useLayoutEffect, useRef } from 'react';
import { Button as AriaButton } from '../../components/ui/data-tree';
import { ChevronRightIcon, HashIcon, CircleFadingPlusIcon } from 'lucide-react';
import type { ConnectionGroupView as ConnectionFolder } from '@contracts';
import { Badge } from '../../components/ui/badge';

import { Input } from '../../components/ui/input';
import { FieldError } from '../../components/ui/field';
import { TruncatedText } from '../../components/ui/tooltip';
import { useConnections } from './ConnectionProvider';
import { errorText, text } from './text';

export function ConnectionGroupHeader({
  folder,
  name,
  expanded,
  count,
  surfaceId,
}: {
  folder?: ConnectionFolder;
  name: string;
  expanded: boolean;
  count: number | null;
  surfaceId: string;
}) {
  const state = useConnections();
  const t = (key: Parameters<typeof text>[1]) => text(state.snapshot.appearance.locale, key);
  const edit =
    state.groupEditing?.id === folder?.id && state.groupEditing?.surfaceId === surfaceId
      ? state.groupEditing
      : null;
  const input = useRef<HTMLInputElement>(null);
  const composing = useRef(false);
  const cancelled = useRef(false);
  const editing = !!edit;
  useLayoutEffect(() => {
    if (!editing) return;
    cancelled.current = false;
    input.current?.scrollIntoView({ block: 'nearest' });
    input.current?.focus();
    input.current?.select();
  }, [editing]);
  const commit = () => {
    if (!cancelled.current && edit && folder && !state.groupBusy)
      void state.renameGroup(folder, edit.draft);
  };
  return (
    <div className="connection-group-heading">
      <div className="connection-group-heading-content">
        <OperationIndicator
          owner={folder ? 'group:' + folder.id : 'navigation:groups'}
          icon={<HashIcon className="size-3.5 shrink-0 text-muted-foreground" />}
          className="size-3.5"
        />
        {edit ? (
          <>
            <Input
              ref={input}
              variant="inline"
              className="min-w-0 flex-1"
              aria-label={t('folderName')}
              aria-invalid={!!edit.error}
              value={edit.draft}
              readOnly={state.groupBusy}
              onChange={(e) =>
                state.setGroupEditing(
                  (current) =>
                    current && {
                      ...current,
                      draft: e.target.value,
                      error: null,
                    },
                )
              }
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => e.stopPropagation()}
              onCompositionStart={() => {
                composing.current = true;
              }}
              onCompositionEnd={() => {
                composing.current = false;
              }}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === 'Escape') {
                  e.preventDefault();
                  cancelled.current = true;
                  state.setGroupEditing(null);
                } else if (
                  e.key === 'Enter' &&
                  !composing.current &&
                  !e.nativeEvent.isComposing &&
                  e.nativeEvent.keyCode !== 229
                ) {
                  e.preventDefault();
                  commit();
                }
              }}
              onBlur={commit}
            />
            <AriaButton slot="chevron" className="connection-chevron">
              <ChevronRightIcon
                className={
                  expanded
                    ? 'size-3.5 text-muted-foreground rotate-90'
                    : 'size-3.5 text-muted-foreground'
                }
              />
            </AriaButton>
          </>
        ) : (
          <AriaButton slot="chevron" appearance="sidebar-group" className="connection-group-toggle">
            <TruncatedText className="connection-tree-name">{name}</TruncatedText>
            <ChevronRightIcon
              className={
                expanded
                  ? 'size-3.5 shrink-0 text-muted-foreground rotate-90'
                  : 'size-3.5 shrink-0 text-muted-foreground'
              }
            />
          </AriaButton>
        )}
        {count !== null ? (
          <Badge variant="sidebar" aria-label={`${t('groupCount')}: ${count}`}>
            {count}
          </Badge>
        ) : null}
        <IconButton
          size="icon-xs"
          variant="titlebar"
          className="connection-group-create"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            state.showNew(folder?.id ?? null);
          }}
          label={t('new')}
          tooltip={<>{t('new')}</>}
        >
          <CircleFadingPlusIcon />
        </IconButton>
      </div>
      {edit?.error ? (
        <FieldError>{errorText(state.snapshot.appearance.locale, edit.error)}</FieldError>
      ) : null}
    </div>
  );
}
