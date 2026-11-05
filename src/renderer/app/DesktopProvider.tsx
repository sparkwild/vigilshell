import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { setLocale } from '../../shared/i18n/generated/runtime.js';
import { translate, type MessageKey } from '../../shared/i18n/messages';
import type { DesktopStore } from '../shared/platform/desktop-store';
const DesktopContext = createContext<DesktopStore | null>(null);
export function DesktopProvider({ store, children }: { store: DesktopStore; children: ReactNode }) {
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot);
  useLayoutEffect(() => {
    applyDisplay(snapshot.appearance);
  }, [snapshot.appearance]);
  return <DesktopContext.Provider value={store}>{children}</DesktopContext.Provider>;
}
export function applyDisplay(display: ReturnType<DesktopStore['getSnapshot']>['appearance']) {
  document.documentElement.classList.toggle('dark', display.theme === 'dark');
  document.documentElement.style.colorScheme = display.theme;
  document.documentElement.lang = display.locale;
  document.documentElement.dir = 'ltr';
  document.documentElement.dataset.platform = /Mac/.test(navigator.userAgent) ? 'macos' : 'other';
  // 显式 React 订阅负责重绘，Paraglide 本身不负责页面刷新或 DOM 属性。
  void setLocale(display.locale, { reload: false });
}
export function useDesktop() {
  const store = useContext(DesktopContext);
  if (!store) throw Error('missing_desktop_provider');
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot);
  const locale = snapshot.appearance.locale;
  const t = useCallback(
    (key: MessageKey, inputs?: Record<string, string | number>) => translate(key, locale, inputs),
    [locale],
  );
  return { store, snapshot, t };
}
