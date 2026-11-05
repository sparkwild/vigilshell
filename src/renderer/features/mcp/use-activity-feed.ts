import { useEffect, useRef, useState } from 'react';
import type { ActivitySummary, ActivitySummaryFilter, RuntimeStatus } from '@contracts';
import { eventSummary, mergeActivity, taskSummary } from './activity-feed';

type Names = Pick<ActivitySummary, 'connectionName' | 'entryName' | 'entryId'>;
export function useActivityFeed(filter: ActivitySummaryFilter, runtime?: RuntimeStatus) {
  const api = window.vigilshell.mcp;
  const runtimeId = runtime?.instanceId;
  const runtimePhase = runtime?.status;
  const [items, setItems] = useState<ActivitySummary[]>([]);
  const [more, setMore] = useState(false);
  const [next, setNext] = useState<ActivitySummaryFilter>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const previousRuntime = useRef<string | null | undefined>(undefined);
  const revisions = useRef(new Map<string, number>());
  const names = useRef(new Map<string, Names>());
  useEffect(() => {
    if (previousRuntime.current !== runtimeId) {
      revisions.current.clear();
      names.current.clear();
      previousRuntime.current = runtimeId;
    }
    if (runtimePhase && runtimePhase !== 'ready') {
      setBusy(runtimePhase === 'starting');
      setError(runtimePhase === 'unavailable' ? 'runtime_unavailable' : null);
      return;
    }
    let alive = true;
    // Subscribe before the snapshot. Pushes received during the read win over its older values.
    const duringRead = new Map<string, ActivitySummary>();
    const metadataReads = new Set<string>();
    const enrich = (item: ActivitySummary) => ({
      ...item,
      entryId: item.entryId || names.current.get(item.sessionId)?.entryId,
      connectionName:
        item.connectionName || names.current.get(item.sessionId)?.connectionName || '',
      entryName: item.entryName || names.current.get(item.sessionId)?.entryName || '',
    });
    const put = (item: ActivitySummary) => {
      const value = enrich(item);
      duringRead.set(value.id, value);
      setItems((old) => mergeActivity(old, [value], filter));
    };
    const remember = (sessionId: string, value: Names) => {
      names.current.set(sessionId, value);
      for (const [id, item] of duringRead)
        if (item.sessionId === sessionId) duringRead.set(id, enrich(item));
      setItems((old) => mergeActivity(old, [...duringRead.values()], filter));
    };
    const unsubscribe = api.onChanged((event) => {
      if (event.kind === 'context') {
        remember(event.context.id, event.context);
      } else if (event.kind === 'task') {
        const task = event.task;
        if ((task.revision ?? 0) <= (revisions.current.get(task.id) ?? -1)) return;
        revisions.current.set(task.id, task.revision ?? 0);
        remember(task.contextId, {
          entryId: task.entryId,
          connectionName: task.connectionName,
          entryName: task.entryName ?? '',
        });
        put(taskSummary(task));
      } else if (event.kind === 'activity') {
        const item = eventSummary(event.record);
        if (!item) return;
        put(item);
        if (!names.current.has(item.sessionId) && !metadataReads.has(item.sessionId)) {
          metadataReads.add(item.sessionId);
          // Only resolve missing labels for this pushed session, never reload the list.
          void api
            .request({ action: 'recordings', filter: { sessionId: item.sessionId, offset: 0 } })
            .then((result) => {
              if (!alive || !result.ok || result.reply?.kind !== 'recordings') return;
              const session = result.reply.sessions[0];
              if (session) remember(item.sessionId, session);
            })
            .catch(() => {
              /* The activity remains visible even when labels are unavailable. */
            });
        }
      }
    });
    setBusy(true);
    if (!filter.before) setItems([]);
    void api
      .request({ action: 'activitySummaries', filter })
      .then((result) => {
        if (!alive) return;
        if (!result.ok) {
          setError(result.code);
          return;
        }
        if (result.reply?.kind !== 'activitySummaries') {
          setError('invalid_response');
          return;
        }
        const page = result.reply.items;
        for (const item of page)
          if (!names.current.has(item.sessionId)) names.current.set(item.sessionId, item);
        setItems((old) =>
          mergeActivity(
            filter.before ? old.concat(page) : page,
            [...duringRead.values()].map(enrich),
            filter,
          ),
        );
        const last = page.at(-1);
        setNext(last ? { before: last.id, beforeTime: last.createdAt } : {});
        setMore(result.reply.more);
        setError(null);
      })
      .catch(() => {
        if (alive) setError('runtime_unavailable');
      })
      .finally(() => {
        if (alive) setBusy(false);
      });
    return () => {
      alive = false;
      unsubscribe();
    };
  }, [filter, runtimeId, runtimePhase, api]);
  return { items, more, next, busy, error };
}
