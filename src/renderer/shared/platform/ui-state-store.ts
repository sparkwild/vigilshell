import { useSyncExternalStore } from 'react';
import type { UiStateData } from '@contracts';
import type { UiStateNotice, UiStatePatch } from '../../../shared/ui-state';
type View = UiStateNotice & { ready: boolean };
const views = new Map<string, View>(),
  listeners = new Set<() => void>(),
  loads = new Map<string, Promise<void>>();
let subscribed = false;
function emit() {
  listeners.forEach((fn) => fn());
}
function accept(notice: UiStateNotice) {
  views.set(notice.scope, {
    ...notice,
    ready: true,
  });
  emit();
}
function snapshot(scope: string): View {
  let value = views.get(scope);
  if (!value) {
    value = { scope, data: null, error: null, ready: false };
    views.set(scope, value);
  }
  return value;
}
export function loadUiState(scope: string): Promise<void> {
  if (snapshot(scope).ready) return Promise.resolve();
  const pending = loads.get(scope);
  if (pending) return pending;
  if (!subscribed && window.vigilshell.onUiStateChanged) {
    window.vigilshell.onUiStateChanged(accept);
    subscribed = true;
  }
  const call = window.vigilshell.getUiState?.(scope);
  if (!call) return Promise.resolve();
  const work = call
    .then(accept)
    .catch(() => accept({ scope, data: null, error: 'storage_unavailable' }))
    .finally(() => loads.delete(scope));
  loads.set(scope, work);
  return work;
}
export function readUiState(scope: string) {
  return snapshot(scope);
}
export async function updateUiState(scope: string, patch: UiStatePatch) {
  await loadUiState(scope);
  const old = snapshot(scope);
  accept({
    ...old,
    data: { ...old.data, ...patch } as UiStateData,
    error: old.error,
  });
  try {
    const result = await window.vigilshell.updateUiState(scope, patch);
    if (result.error) accept(result);
  } catch {
    accept({ ...snapshot(scope), error: 'storage_unavailable' });
  }
}
export function useUiStateErrors() {
  // Keep a stable primitive snapshot rather than allocating an array on every render.
  const scopes = useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    () =>
      [...views.values()]
        .filter((v) => v.error)
        .map((v) => v.scope)
        .join('\n'),
  );
  return scopes ? scopes.split('\n') : [];
}
export async function retryUiState(scope: string) {
  try {
    accept(await window.vigilshell.retryUiState(scope));
  } catch {
    accept({ ...snapshot(scope), error: 'storage_unavailable' });
  }
}
