import type { TerminalSessionView, ConnectionDetails, TrustView } from '@contracts';
import {
  HostIdentityDialog,
  TransferCloseDialog,
  type CloseTransferChoice,
} from '../server-tools/tool-dialogs';
import { ConnectionOpenController } from './connection-open-controller';
import { ConnectionOpenDialog } from './ConnectionOpenDialog';
import { useConfirmation, useConfirmationState } from '../../shared/ConfirmationProvider';
import {
  useEffect,
  useCallback,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { CopyIcon } from 'lucide-react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Badge } from '../../components/ui/badge';
import { Alert, AlertDescription } from '../../components/ui/alert';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../../components/ui/dialog';
import { useWorkspace, errorCode } from './workspace-context';
import { WorkspaceUiContext } from './ui-context';
import { ConnectionEditor } from './connection-editor';
import { terminalIsActive } from '../../../shared/workspace-api';
import type { Resource } from './workspace-store';
export function WorkspaceUi({
  active,
  navigationWidth,
  commandsActive = active,
  beforeConnectionAction,
  children,
}: {
  active: boolean;
  navigationWidth: number;
  commandsActive?: boolean;
  beforeConnectionAction?: () => Promise<boolean>;
  children: ReactNode;
}) {
  const { t, store: desktopStore } = useDesktop();
  const { store, state } = useWorkspace();
  const confirm = useConfirmation();
  const { open: confirmation, cancel: cancelConfirmation } = useConfirmationState();
  const [dirty, setDirty] = useState(false);
  const [toolDialogCloser, setToolDialogCloser] = useState<(() => void) | null>(null);
  const setToolDialogClose = useCallback(
    (close: (() => void) | null) => setToolDialogCloser(() => close),
    [],
  );
  const [identity, setIdentity] = useState<{
    connection: ConnectionDetails;
    trust: TrustView | null;
  } | null>(null);
  const [transferClose, setTransferClose] = useState<TerminalSessionView | null>(null);
  const closeTransferRequest = useRef<{
    session: string;
    promise: Promise<CloseTransferChoice>;
    resolve(choice: CloseTransferChoice): void;
  } | null>(null);
  const finishTransferClose = (choice: CloseTransferChoice) => {
    const request = closeTransferRequest.current;
    closeTransferRequest.current = null;
    setTransferClose(null);
    request?.resolve(choice);
  };
  useEffect(() => {
    store.setTerminalCloseGuard((session) => {
      if (closeTransferRequest.current?.session === session.id)
        return closeTransferRequest.current.promise;
      closeTransferRequest.current?.resolve('cancel');
      let resolve!: (choice: CloseTransferChoice) => void;
      const promise = new Promise<CloseTransferChoice>((done) => {
        resolve = done;
      });
      closeTransferRequest.current = { session: session.id, promise, resolve };
      setTransferClose(session);
      return promise;
    });
    return () => {
      store.setTerminalCloseGuard(null);
      closeTransferRequest.current?.resolve('cancel');
      closeTransferRequest.current = null;
    };
  }, [store]);
  useEffect(() => {
    if (!state.transportReady) {
      finishTransferClose('cancel');
      setIdentity(null);
    }
  }, [state.runtimeId, state.transportReady]);
  const [connectionOpener] = useState(
    () =>
      new ConnectionOpenController(
        store,
        () => {},
        () => desktopStore.getSnapshot().preferences?.connectionOpenBehavior ?? 'ask',
      ),
  );
  const connectionChoice = useSyncExternalStore(
    connectionOpener.subscribe,
    connectionOpener.getSnapshot,
  );
  useEffect(() => connectionOpener.connect(), [connectionOpener]);
  useEffect(() => {
    if (!active) connectionOpener.cancel();
  }, [active, connectionOpener]);
  const [paste, setPaste] = useState<{
    text: string;
    resolve: (accept: boolean) => void;
  } | null>(null);
  const closeEditor = useRef<(() => Promise<void>) | null>(null),
    cancelButton = useRef<HTMLButtonElement | null>(null);
  const challenge = [...state.operations.values()].find(
    (o) => o.phase === 'trust' && o.challenge,
  )?.challenge;
  function deleteResource(resource: Resource) {
    void (async () => {
      try {
        {
          let list: import('@contracts').WorkspaceReply | null = null;
          const result = await store.run({
            owner: 'connection-read:' + resource.id,
            action: 'ws_loading',
            label: store.connectionLabel(resource.id),
            execute: async () => {
              const [connection, sessions] = await Promise.all([
                store.call('getConnection', { id: resource.id }),
                store.call('listTerminals', {}),
              ]);
              list = sessions;
              return connection;
            },
          });
          const sessionReply = list as import('@contracts').WorkspaceReply | null;
          if (result.kind !== 'connection' || sessionReply?.kind !== 'sessions') return;
          const c = result.connection,
            sessions = sessionReply.sessions.filter(
              (s) => s.connectionId === c.id && terminalIsActive(s),
            );
          await confirm({
            tone: 'danger',
            title: t('ws_delete_connection'),
            description: t('ws_delete_connection_body'),
            confirmLabel: t('ws_delete_connection'),
            cancelLabel: t('ws_cancel'),
            detail: (
              <div className="flex flex-col gap-2">
                <strong>{c.config.name}</strong>
                <div>
                  {t('ws_authentication')}{' '}
                  <Badge variant="secondary">
                    {t(c.credentialSaved ? 'ws_credential_saved' : 'ws_credential_missing')}
                  </Badge>
                </div>
                {c.credentialLabel ? <p className="break-all">{c.credentialLabel}</p> : null}
                <div>
                  {t('ws_affected_sessions')} <Badge variant="secondary">{sessions.length}</Badge>
                </div>
                {sessions.map((s) => (
                  <div key={s.id}>{s.name}</div>
                ))}
              </div>
            ),
            onConfirm: async () => {
              try {
                await store.removeConnection(
                  c.id,
                  c.version,
                  sessions.map((s) => s.id),
                );
                return true;
              } catch (error) {
                if (!store.feedback.getSnapshot().operations.some((op) => op.error))
                  store.report(errorCode(error));
                return false;
              }
            },
          });
        }
      } catch {
        /* The object-scoped operation retains preparation failures. */
      }
    })();
  }
  useEffect(() => {
    window.vigilshell.setWorkspaceUi({
      dirty,
      modalOpen:
        !!state.editor ||
        !!confirmation ||
        !!paste ||
        !!connectionChoice ||
        !!identity ||
        !!transferClose ||
        !!toolDialogCloser,
      activeSessionId: state.activeTab === 'overview' ? null : state.activeTab,
      active,
    });
  }, [
    dirty,
    state.editor,
    state.activeTab,
    confirmation,
    paste,
    active,
    connectionChoice,
    identity,
    transferClose,
    toolDialogCloser,
  ]);
  useEffect(
    () =>
      window.vigilshell.onWorkspaceCommand((command) => {
        if (!commandsActive) return;
        if (command === 'close') {
          if (transferClose) {
            finishTransferClose('cancel');
          } else if (toolDialogCloser) {
            toolDialogCloser();
          } else if (document.activeElement?.closest('[data-transfer-window]')) {
            const id = document.activeElement
              .closest('[data-transfer-window]')
              ?.getAttribute('data-transfer-window');
            if (id) store.closeTransfers(id);
          } else if (identity) {
            setIdentity(null);
          } else if (connectionChoice) {
            connectionOpener.cancel();
          } else if (paste) {
            paste.resolve(false);
            setPaste(null);
          } else if (confirmation) {
            cancelConfirmation();
          } else if (
            state.editor &&
            (state.activeTab === 'overview' ||
              document.activeElement?.closest('.connection-editor'))
          ) {
            void closeEditor.current?.();
          } else if (state.activeTab !== 'overview')
            void store.closeView(state.activeTab).catch((e) => store.report(errorCode(e)));
          else window.vigilshell.hideWindow();
        } else if (command === 'new') {
          if (state.editor) {
            document.querySelector<HTMLInputElement>('[role="dialog"] input')?.focus();
            return;
          }
          if (confirmation || paste || connectionChoice || identity || transferClose) return;
          store.newConnection(state.details?.groupId ?? null);
        } else if (command === 'find') {
          if (
            state.editor ||
            confirmation ||
            paste ||
            connectionChoice ||
            identity ||
            transferClose
          )
            return;
          window.dispatchEvent(
            new CustomEvent('vigilshell:find', {
              detail: document.activeElement?.closest('.connection-navigation')
                ? 'overview'
                : state.activeTab,
            }),
          );
        }
      }),
    [
      commandsActive,
      confirmation,
      cancelConfirmation,
      identity,
      transferClose,
      toolDialogCloser,
      connectionChoice,
      connectionOpener,
      paste,
      state.editor,
      state.activeTab,
      state.selected,
      state.details,
      store,
    ],
  );
  useEffect(() => {
    if (!state.editor) setDirty(false);
  }, [state.editor]);
  const actions = {
    active,
    navigationWidth,
    beforeConnectionAction,
    openConnection: async (id: string, signal?: AbortSignal) => {
      if (beforeConnectionAction && !(await beforeConnectionAction())) return null;
      return connectionOpener.openConnection(id, signal);
    },
    setToolDialogClose,
    showHostIdentity: (id: string) => {
      void store
        .call('getConnection', { id })
        .then((reply) => {
          if (reply.kind === 'connection')
            setIdentity({ connection: reply.connection, trust: reply.trust });
        })
        .catch((e) => store.report(errorCode(e)));
    },
    retryCredentialCleanup: (id: string) => {
      void store
        .mutation('reconcileCredentials', { id })
        .then(() => store.refresh())
        .catch((e) => store.report(errorCode(e)));
    },
    deleteResource,
    paste: (text: string) => new Promise<boolean>((resolve) => setPaste({ text, resolve })),
    setDirty,
    setCloseEditor: (close: (() => Promise<void>) | null) => {
      closeEditor.current = close;
      store.setEditorCloser(close);
    },
  };
  return (
    <WorkspaceUiContext.Provider value={actions}>
      {children}
      <HostIdentityDialog value={identity} close={() => setIdentity(null)} update={setIdentity} />
      <TransferCloseDialog session={transferClose} choose={finishTransferClose} />
      <ConnectionOpenDialog
        view={connectionChoice}
        choose={connectionOpener.choose}
        focusReusedTerminal={() => store.terminals.get(store.getSnapshot().activeTab)?.focus()}
      />
      {state.editor?.kind === 'connection' ? (
        <ConnectionEditor key={state.editor.token} editor={state.editor} />
      ) : null}
      <Dialog
        open={active && !!paste}
        onOpenChange={(open) => {
          if (!open) {
            paste?.resolve(false);
            setPaste(null);
          }
        }}
      >
        <DialogContent size="wide" closeLabel={t('ws_close')}>
          <DialogHeader>
            <DialogTitle>{t('ws_paste_title')}</DialogTitle>
            <DialogDescription>{t('ws_paste_body')}</DialogDescription>
          </DialogHeader>
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-all">
            {paste?.text.slice(0, 16000)}
          </pre>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                paste?.resolve(false);
                setPaste(null);
              }}
            >
              {t('ws_cancel')}
            </Button>
            <Button
              onClick={() => {
                paste?.resolve(true);
                setPaste(null);
              }}
            >
              {t('ws_paste_confirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog
        open={active && !!challenge && !confirmation && !paste && !connectionChoice}
        onOpenChange={(open) => {
          if (!open && challenge)
            void store
              .call('rejectTrust', { challengeId: challenge.id })
              .catch((e) => store.report(errorCode(e)));
        }}
      >
        <DialogContent size="wide" showCloseButton={false} initialFocus={cancelButton}>
          <DialogHeader>
            <DialogTitle>
              {t(challenge?.previousFingerprint ? 'ws_trust_changed' : 'ws_trust_first')}
            </DialogTitle>
            <DialogDescription>{t('ws_trust_body')}</DialogDescription>
          </DialogHeader>
          {challenge ? (
            <div className="flex flex-col gap-3">
              <div className="break-all">
                {challenge.name}
                <br />
                {challenge.host}:{challenge.port}
              </div>
              <div>
                {t('ws_algorithm')}: {challenge.algorithm}
              </div>
              {challenge.previousFingerprint ? (
                <div>
                  <div>{t('ws_previous_fingerprint')}</div>
                  <code className="break-all">{challenge.previousFingerprint}</code>
                </div>
              ) : null}
              <div className="flex flex-col gap-1">
                <div>{t('ws_fingerprint')}</div>
                <div className="flex items-center gap-2">
                  <code className="min-w-0 break-all">{challenge.fingerprint}</code>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t('ws_copy')}
                    onClick={() => void window.vigilshell.copyText(challenge.fingerprint)}
                  >
                    <CopyIcon />
                  </Button>
                </div>
              </div>
              {challenge.temporary ? (
                <Alert>
                  <AlertDescription>{t('ws_trust_temporary')}</AlertDescription>
                </Alert>
              ) : null}
            </div>
          ) : null}
          <DialogFooter>
            <Button
              ref={cancelButton}
              variant="outline"
              onClick={() => {
                if (challenge)
                  void store
                    .call('rejectTrust', { challengeId: challenge.id })
                    .catch((e) => store.report(errorCode(e)));
              }}
            >
              {t('ws_cancel')}
            </Button>
            <Button
              onClick={() => {
                if (challenge)
                  void store
                    .call('confirmTrust', { challengeId: challenge.id })
                    .catch((e) => store.report(errorCode(e)));
              }}
            >
              {t('ws_trust_continue')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </WorkspaceUiContext.Provider>
  );
}
