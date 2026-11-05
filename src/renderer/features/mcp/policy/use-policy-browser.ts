import { useCallback, useEffect, useRef, useState } from 'react';
import type { ConnectionSummary, PolicyResourceQuery, PolicyResourcePage } from '@contracts';

/** 浏览租约只属于当前选择器；关闭后到达的握手结果也必须释放。 */
export function usePolicyBrowser(connection: ConnectionSummary) {
  const lease = useRef<Promise<string> | null>(null);
  const alive = useRef(false);
  const generation = useRef(0);
  const [pending, setPending] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const { id, revision } = connection;
  useEffect(() => {
    alive.current = true;
    const current = ++generation.current;
    return () => {
      alive.current = false;
      generation.current = current + 1;
      const closing = lease.current;
      lease.current = null;
      void closing
        ?.then((browserId) =>
          window.vigilshell.mcp.request({ action: 'closePolicyBrowser', browserId }),
        )
        .catch(() => {});
    };
  }, [id, revision]);
  const read = useCallback(
    async (query: PolicyResourceQuery): Promise<PolicyResourcePage | null> => {
      if (!alive.current) return null;
      const current = generation.current;
      setPending((n) => n + 1);
      setError(null);
      let openedBrowser = false;
      try {
        if (!lease.current) {
          lease.current = window.vigilshell.mcp
            .request({
              action: 'openPolicyBrowser',
              connectionId: id,
              connectionRevision: revision,
            })
            .then((result) => {
              if (!result.ok) throw Error(result.code);
              if (result.reply?.kind !== 'policyBrowser') throw Error('invalid_response');
              return result.reply.id;
            });
        }
        const opened = lease.current;
        const browserId = await opened;
        if (!alive.current || lease.current !== opened) return null;
        openedBrowser = true;
        const result = await window.vigilshell.mcp.request({
          action: 'readPolicyResources',
          browserId,
          query,
        });
        if (!alive.current || lease.current !== opened) return null;
        if (!result.ok) throw Error(result.code);
        if (result.reply?.kind !== 'policyResources') throw Error('invalid_response');
        return result.reply.page;
      } catch (cause) {
        if (alive.current && generation.current === current) {
          setError(cause instanceof Error ? cause.message : 'runtime_unavailable');
          const code = cause instanceof Error ? cause.message : 'runtime_unavailable';
          if (
            !openedBrowser ||
            [
              'resource_browser_expired',
              'connection_changed',
              'runtime_unavailable',
              'invalid_response',
            ].includes(code)
          ) {
            const closing = lease.current;
            lease.current = null;
            void closing
              ?.then((browserId) =>
                window.vigilshell.mcp.request({ action: 'closePolicyBrowser', browserId }),
              )
              .catch(() => {});
          }
        }
        return null;
      } finally {
        if (alive.current && generation.current === current) setPending((n) => Math.max(0, n - 1));
      }
    },
    [id, revision],
  );
  return { read, busy: pending > 0, error };
}
