import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { ActionButton } from './ActionControls';
import { ConfirmationDialog } from './ConfirmationDialog';
import type { DecisionOptions } from './decision';

export type ConfirmationOptions = Omit<DecisionOptions, 'actions' | 'onAction'> & {
  confirmLabel: string;
  cancelLabel: string;
  /** 返回 false 保留弹窗；抛出错误拒绝请求。 */
  onConfirm?: () => Promise<boolean>;
};
type Request = DecisionOptions & {
  cancelId: string;
  cancelLabel: string;
  resolve(value: string): void;
  reject(error: unknown): void;
  signal: AbortSignal;
  detach(): void;
};
const Context = createContext<
  ((options: DecisionOptions, signal: AbortSignal) => Promise<string>) | null
>(null);
const StateContext = createContext<{ open: boolean; busy: boolean; cancel(): void } | null>(null);

export function ConfirmationProvider({ children }: { children: ReactNode }) {
  const requests = useRef<Request[]>([]);
  const executing = useRef<Request | null>(null);
  const mounted = useRef(false);
  const [current, setCurrent] = useState<Request | null>(null);
  const [busy, setBusy] = useState(false);
  const finish = useCallback((request: Request, value: string) => {
    const index = requests.current.indexOf(request);
    if (index < 0) return;
    requests.current.splice(index, 1);
    request.detach();
    request.resolve(value);
    if (mounted.current) setCurrent(requests.current[0] ?? null);
  }, []);
  const decide = useCallback(
    (options: DecisionOptions, signal: AbortSignal) =>
      new Promise<string>((resolve, reject) => {
        const cancel = options.actions.find((action) => action.role === 'cancel');
        if (
          !cancel ||
          cancel.disabled ||
          options.actions.filter((a) => a.role === 'cancel').length !== 1 ||
          new Set(options.actions.map((a) => a.id)).size !== options.actions.length
        ) {
          reject(new Error('A decision requires one cancel action and unique action ids'));
          return;
        }
        if (!mounted.current || signal.aborted) {
          resolve(cancel.id);
          return;
        }
        const abort = () => finish(request, cancel.id);
        const request: Request = {
          ...options,
          cancelId: cancel.id,
          cancelLabel: cancel.label,
          resolve,
          reject,
          signal,
          detach: () => signal.removeEventListener('abort', abort),
        };
        signal.addEventListener('abort', abort, { once: true });
        requests.current.push(request);
        if (requests.current.length === 1) setCurrent(request);
      }),
    [finish],
  );
  const cancel = useCallback(() => {
    const request = requests.current[0];
    if (request && !executing.current) finish(request, request.cancelId);
  }, [finish]);
  const select = async (id: string) => {
    if (executing.current || !current || current.signal.aborted) return;
    const request = current;
    const action = request.actions.find((a) => a.id === id);
    if (!action || action.disabled) return;
    executing.current = request;
    setBusy(true);
    try {
      if (!request.onAction || (await request.onAction(id))) finish(request, id);
    } catch (error) {
      request.reject(error);
      finish(request, request.cancelId);
    } finally {
      executing.current = null;
      if (mounted.current) setBusy(false);
    }
  };
  useEffect(() => {
    mounted.current = true;
    const queue = requests.current;
    return () => {
      mounted.current = false;
      for (const request of queue.splice(0)) {
        request.detach();
        request.resolve(request.cancelId);
      }
    };
  }, []);
  return (
    <Context value={decide}>
      <StateContext value={{ open: current !== null, busy, cancel }}>
        {children}
        <ConfirmationDialog
          focusToken={current}
          open={!!current}
          title={current?.title ?? ''}
          description={current?.description ?? ''}
          detail={current?.detail}
          tone={current?.tone ?? 'warning'}
          size={current?.size}
          cancelLabel={current?.cancelLabel ?? ''}
          busy={busy}
          onCancel={cancel}
        >
          {current?.actions
            .filter((a) => a.role !== 'cancel')
            .map((action) => (
              <ActionButton
                key={action.id}
                variant={
                  action.tone === 'destructive'
                    ? 'destructive'
                    : action.role === 'primary'
                      ? 'default'
                      : 'outline'
                }
                disabled={busy || action.disabled}
                disabledReason={action.disabledReason}
                onClick={() => void select(action.id)}
              >
                {action.label}
              </ActionButton>
            ))}
        </ConfirmationDialog>
      </StateContext>
    </Context>
  );
}

export function useDecision() {
  const decide = useContext(Context);
  if (!decide) throw new Error('ConfirmationProvider is required');
  const controllers = useRef(new Set<AbortController>());
  useEffect(() => {
    const active = controllers.current;
    return () => {
      for (const controller of active) controller.abort();
      active.clear();
    };
  }, []);
  return useCallback(
    (options: DecisionOptions) => {
      const controller = new AbortController();
      controllers.current.add(controller);
      return decide(options, controller.signal).finally(() =>
        controllers.current.delete(controller),
      );
    },
    [decide],
  );
}
export function useConfirmation() {
  const decide = useDecision();
  return useCallback(
    (options: ConfirmationOptions) =>
      decide({
        ...options,
        actions: [
          { id: 'cancel', label: options.cancelLabel, role: 'cancel' },
          {
            id: 'confirm',
            label: options.confirmLabel,
            role: 'primary',
            tone: options.tone === 'neutral' ? 'default' : 'destructive',
          },
        ],
        onAction: options.onConfirm,
      }).then((value) => value === 'confirm'),
    [decide],
  );
}
export function useConfirmationState() {
  const state = useContext(StateContext);
  if (!state) throw new Error('ConfirmationProvider is required');
  return state;
}
