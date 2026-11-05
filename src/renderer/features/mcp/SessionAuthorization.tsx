import { useEffect, useRef, useState } from 'react';
import { useWorkspace } from '../connections/workspace-context';
import { useMcp } from './model';
import { useAuthorizationForeground } from './use-authorization-foreground';

export function SessionAuthorization() {
  const { state } = useWorkspace();
  const { snapshot, issue } = useMcp();
  const foreground = useAuthorizationForeground();
  const attempted = useRef(new Set<string>());
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const contexts = snapshot?.contexts;
  const ready = state.transportReady && !!state.runtimeId && !snapshot?.error;

  useEffect(() => {
    if (!ready || !foreground || busy || inFlight.current) return;
    const next = contexts
      ?.filter((context) => context.phase === 'waitingUser')
      .sort((a, b) => a.createdAt - b.createdAt || a.id.localeCompare(b.id))
      .find((context) => !attempted.current.has(`${state.runtimeId}:${context.id}`));
    if (!next) return;
    attempted.current.add(`${state.runtimeId}:${next.id}`);
    inFlight.current = true;
    setBusy(true);
    void issue({ action: 'approveConnection', contextId: next.id }).finally(() => {
      inFlight.current = false;
      setBusy(false);
    });
  }, [ready, foreground, busy, contexts, state.runtimeId, issue]);
  return null;
}
