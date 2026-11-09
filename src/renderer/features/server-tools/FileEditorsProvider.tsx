import { createContext, useContext, useSyncExternalStore, type ReactNode } from 'react';
import { FileEditorsStore } from './file-editors-store';
const Context = createContext<FileEditorsStore | null>(null);
export function FileEditorsProvider({
  store,
  children,
}: {
  store: FileEditorsStore;
  children: ReactNode;
}) {
  return <Context value={store}>{children}</Context>;
}
export function useFileEditors() {
  const store = useContext(Context);
  if (!store) throw Error('missing_file_editors_provider');
  const state = useSyncExternalStore(store.subscribe, store.getSnapshot);
  return { store, ...state };
}
