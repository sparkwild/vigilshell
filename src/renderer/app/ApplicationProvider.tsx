import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import type { UpdateSnapshot } from '@contracts';
const initial: UpdateSnapshot = { phase: 'idle', release: null };
const ApplicationContext = createContext(initial);
export function ApplicationProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState(initial);
  useEffect(() => {
    const api = window.vigilshell.application;
    if (!api) return;
    let active = true,
      received = false;
    const stop = api.onChanged((value) => {
      received = true;
      setState(value);
    });
    void api
      .state()
      .then((value) => {
        if (active && !received) setState(value);
      })
      .catch(() => {});
    return () => {
      active = false;
      stop();
    };
  }, []);
  return <ApplicationContext.Provider value={state}>{children}</ApplicationContext.Provider>;
}

export function useApplicationState() {
  return useContext(ApplicationContext);
}
