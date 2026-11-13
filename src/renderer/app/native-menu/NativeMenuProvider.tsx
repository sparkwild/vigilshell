import { createContext, useContext, useEffect, useId, useState, type ReactNode } from 'react';
import type { NativeMenuState } from '@contracts';
import { NativeMenuRegistry, type NativeMenuActions } from './registry';

const Context = createContext<NativeMenuRegistry | null>(null);
function visibleDialog() {
  return [...document.querySelectorAll<HTMLElement>('[role="dialog"], [role="alertdialog"]')]
    .reverse()
    .find(
      (node) =>
        !node.closest(
          '[hidden], [inert], [aria-hidden="true"], [data-ending-style], [data-closed]',
        ),
    );
}
export function NativeMenuProvider({ children }: { children: ReactNode }) {
  const [registry] = useState(() => new NativeMenuRegistry());
  useEffect(() => {
    let mounted = true,
      queued = false,
      previous = '';
    const publish = () => {
      queued = false;
      if (!mounted) return;
      const snapshot: NativeMenuState = registry.snapshot();
      if (visibleDialog()) {
        snapshot.enabled = snapshot.enabled.filter((command) => command === 'close');
        snapshot.closeTarget = 'dialog';
      }
      const json = JSON.stringify(snapshot);
      if (json !== previous) {
        previous = json;
        window.vigilshell.setNativeMenuState?.(snapshot);
      }
    };
    const schedule = () => {
      if (!queued) {
        queued = true;
        queueMicrotask(publish);
      }
    };
    registry.changed = schedule;
    const stop = window.vigilshell.onWorkspaceCommand((command) => {
      if (visibleDialog()) {
        if (command === 'close')
          document.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }),
          );
        return;
      }
      void Promise.resolve()
        .then(() => registry.dispatch(command))
        .catch((error) => console.error('Native menu command:', error));
    });
    // Dialog portals are direct body children; terminal output must not be scanned.
    const observer = new MutationObserver(schedule);
    observer.observe(document.body, { childList: true });
    const attributes = new MutationObserver(schedule);
    attributes.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ['hidden', 'inert', 'aria-hidden', 'data-ending-style', 'data-closed'],
    });
    document.addEventListener('focusin', schedule);
    schedule();
    return () => {
      mounted = false;
      registry.changed = () => {};
      stop();
      observer.disconnect();
      attributes.disconnect();
      document.removeEventListener('focusin', schedule);
    };
  }, [registry]);
  return <Context.Provider value={registry}>{children}</Context.Provider>;
}
export function useNativeMenuActions(actions: NativeMenuActions, priority = 0) {
  const registry = useContext(Context),
    id = useId();
  useEffect(() => {
    registry?.set(id, priority, actions);
    return () => registry?.remove(id);
  }, [registry, id, priority, actions]);
}
