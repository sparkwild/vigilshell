import { useCallback, useEffect, useRef } from 'react';
import type { LeaveGuard } from '../features/mcp/SettingsParts';

export function usePageNavigation() {
  const overlayGuard = useRef<LeaveGuard | null>(null);
  const registerOverlay = useCallback((value: LeaveGuard | null) => {
    overlayGuard.current = value;
  }, []);
  const guard = useRef<LeaveGuard | null>(null),
    pending = useRef(false),
    mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const register = useCallback((value: LeaveGuard | null) => {
    guard.current = value;
  }, []);
  const navigate = useCallback(async (action: () => void | Promise<void>) => {
    if (pending.current) return false;
    pending.current = true;
    try {
      if (overlayGuard.current && !(await overlayGuard.current())) return false;
      if (guard.current && !(await guard.current())) return false;
      if (!mounted.current) return false;
      await action();
      return true;
    } finally {
      pending.current = false;
    }
  }, []);
  return { register, registerOverlay, navigate };
}
