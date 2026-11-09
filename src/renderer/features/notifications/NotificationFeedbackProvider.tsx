import { createContext, useContext, useSyncExternalStore, type ReactNode } from 'react';
import type { NotificationFeedback } from '@contracts';
import { NotificationFeedbackStore } from './feedback-store';
import { useFeedbackAnchor } from '../connections/feedback/use-operation-anchor';
const Context = createContext<NotificationFeedbackStore | null>(null);
const empty: NotificationFeedback[] = [];
const emptySnapshot = () => empty;
const noop = () => {};
const subscribeEmpty = () => noop;
const emptyAnchors = { setAnchor: noop };
export function NotificationFeedbackProvider({
  store,
  children,
}: {
  store: NotificationFeedbackStore;
  children: ReactNode;
}) {
  return <Context value={store}>{children}</Context>;
}
export function useNotificationFeedback() {
  const store = useContext(Context);
  return useSyncExternalStore(
    store?.subscribe ?? subscribeEmpty,
    store?.getSnapshot ?? emptySnapshot,
  );
}
export function useNotificationSurface<T extends HTMLElement = HTMLDivElement>(key: string | null) {
  return useFeedbackAnchor<T>(key, useContext(Context) ?? emptyAnchors);
}
