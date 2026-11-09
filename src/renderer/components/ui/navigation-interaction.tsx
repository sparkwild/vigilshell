import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useState,
  type ReactNode,
} from 'react';

const Context = createContext<{
  held: boolean;
  setHeld: (id: string, held: boolean) => void;
}>({
  held: false,
  setHeld: () => undefined,
});
/** 导航提供通用的交互占用信号，避免悬停面板在拖拽或行内编辑中途消失。 */
export function NavigationInteractionProvider({ children }: { children: ReactNode }) {
  const [holders, setHolders] = useState<Set<string>>(() => new Set());
  const setHeld = useCallback(
    (id: string, held: boolean) =>
      setHolders((current) => {
        if (current.has(id) === held) return current;
        const next = new Set(current);
        if (held) next.add(id);
        else next.delete(id);
        return next;
      }),
    [],
  );
  return (
    <Context.Provider value={{ held: holders.size > 0, setHeld }}>{children}</Context.Provider>
  );
}
export function useNavigationInteraction(held: boolean) {
  const id = useId();
  const { setHeld } = useContext(Context);
  useEffect(() => {
    setHeld(id, held);
    return () => setHeld(id, false);
  }, [held, id, setHeld]);
}
export function useNavigationInteractionHeld() {
  return useContext(Context).held;
}
