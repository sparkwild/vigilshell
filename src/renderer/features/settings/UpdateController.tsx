import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from 'react';
import type { ApplicationInfo } from '@contracts';
import { useApplicationState } from '../../app/ApplicationProvider';
import { UpdateDialog } from './UpdateDialog';
const UpdateContext = createContext<(() => void) | null>(null);
export function useOpenUpdate() {
  const open = useContext(UpdateContext);
  if (!open) throw Error('missing_update_controller');
  return open;
}
/** One dialog for workspace, settings and native menu; opening never starts a download. */
export function UpdateController({ children }: { children: ReactNode }) {
  const state = useApplicationState();
  const hasRelease = !!state.release;
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState(false);
  const [info, setInfo] = useState<ApplicationInfo>();
  const infoFlight = useRef<Promise<void> | null>(null);
  const checking = useRef(false);
  const focus = useRef<HTMLElement | null>(null);
  const show = useCallback(() => {
    if (!open)
      focus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setOpen(true);
    setFailure(false);
    if (!infoFlight.current)
      infoFlight.current = window.vigilshell.application
        .info()
        .then(setInfo)
        .catch(() => {
          infoFlight.current = null;
        });
    if (hasRelease || checking.current) return;
    checking.current = true;
    setPending(true);
    void window.vigilshell.application
      .check()
      .then((result) => {
        setFailure(
          !!result &&
            ['empty', 'networkError', 'rateLimited', 'invalidResponse'].includes(
              result.checkPhase ?? result.phase,
            ),
        );
      })
      .catch(() => setFailure(true))
      .finally(() => {
        checking.current = false;
        setPending(false);
      });
  }, [open, hasRelease]);
  const run = (action: () => Promise<unknown>) => {
    setFailure(false);
    void action().catch(() => setFailure(true));
  };
  return (
    <UpdateContext.Provider value={show}>
      {children}
      <UpdateDialog
        open={open}
        onOpenChange={setOpen}
        state={state}
        info={info}
        pending={pending}
        failure={failure}
        returnFocus={() => (focus.current?.isConnected ? focus.current : false)}
        onDownload={() =>
          run(() =>
            state.canDownload
              ? window.vigilshell.application.download()
              : window.vigilshell.application.open('openRelease'),
          )
        }
        onCancel={() => run(() => window.vigilshell.application.cancel())}
        onInstall={() => run(() => window.vigilshell.application.install())}
        onRelease={() => run(() => window.vigilshell.application.open('openRelease'))}
      />
    </UpdateContext.Provider>
  );
}
