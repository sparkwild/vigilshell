import { useFileEditors } from './FileEditorsProvider';
import { useNotificationSurface } from '../notifications/NotificationFeedbackProvider';
import { useEffect, useRef, useState } from 'react';
import type { FileEditorRequest, FileEditorView } from '@contracts';
import type { MessageKey } from '../../../shared/i18n/messages';
import { userErrorKey } from '../../../shared/i18n/operation-errors';
import { useDesktop } from '../../app/DesktopProvider';
import { useWorkspace } from '../connections/workspace-context';
import { useConfirmation } from '../../shared/ConfirmationProvider';
import {
  Dialog,
  FormDialog,
  DialogForm,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '../../components/ui/dialog';
import { Button } from '../../components/ui/button';
import { Alert, AlertDescription } from '../../components/ui/alert';
import { Empty, EmptyHeader, EmptyDescription } from '../../components/ui/empty';
import { Field, FieldLabel, FieldError } from '../../components/ui/field';
import { Input } from '../../components/ui/input';
import { TruncatedText } from '../../components/ui/tooltip';

export function FileEditorsDialog({
  notificationTarget,
  onUnavailable,
  navigationVersion,
  registerLeaveGuard,
}: {
  navigationVersion?: number;
  registerLeaveGuard?: (guard: (() => Promise<boolean>) | null) => void;
  notificationTarget?: { id: string; version: number };
  onUnavailable?(): void;
}) {
  const { t } = useDesktop(),
    { state } = useWorkspace(),
    confirm = useConfirmation();
  const { store: editingStore, items, loaded, error: readError } = useFileEditors();
  const [open, setOpen] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  const [saveAs, setSaveAs] = useState<{ id: string; path: string; initialPath: string } | null>(
    null,
  );
  const inFlight = useRef(false),
    closingDraft = useRef(false);
  const api = window.vigilshell.fileEditors;
  useEffect(() => {
    if (navigationVersion) {
      setOpen(false);
      setSaveAs(null);
    }
  }, [navigationVersion]);
  useEffect(() => {
    if (!registerLeaveGuard) return;
    registerLeaveGuard(async () => {
      if (inFlight.current) return false;
      if (saveAs && saveAs.path !== saveAs.initialPath) {
        const accepted = await confirm({
          title: t('ws_discard_title'),
          description: t('ui_discard_form_body'),
          tone: 'warning',
          confirmLabel: t('ws_confirm'),
          cancelLabel: t('ws_cancel'),
        });
        if (!accepted) return false;
        setSaveAs(null);
      }
      return true;
    });
    return () => registerLeaveGuard(null);
  }, [registerLeaveGuard, saveAs, confirm, t]);
  useEffect(() => {
    if (notificationTarget) setOpen(true);
  }, [notificationTarget]);
  useEffect(() => {
    if (
      open &&
      notificationTarget &&
      loaded &&
      !items.some((item) => item.id === notificationTarget.id)
    )
      onUnavailable?.();
  }, [open, notificationTarget, items, loaded, onUnavailable]);
  useEffect(
    () =>
      api?.onShow(() => {
        setOpen(true);
        void editingStore.refresh();
      }),
    [api, editingStore],
  );
  async function act(request: FileEditorRequest, handledError?: string) {
    if (!api || inFlight.current) return null;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      const r = await api.request(request);
      editingStore.accept(r.sessions);
      if (!r.ok && (!handledError || r.error !== handledError))
        setError(r.error ?? 'files_unsupported');
      return r;
    } catch {
      setError('runtime_unavailable');
      return null;
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }
  async function destructive(item: FileEditorView, action: 'remote' | 'overwrite' | 'end') {
    if (
      !(await confirm({
        title: t(
          action === 'end'
            ? 'file_editor_discard_title'
            : action === 'remote'
              ? 'file_editor_use_remote'
              : 'file_editor_overwrite',
        ),
        description: t(
          action === 'overwrite'
            ? 'file_editor_overwrite_body'
            : action === 'remote'
              ? 'file_editor_use_remote_body'
              : 'file_editor_discard_body',
        ),
        tone: 'warning',
        confirmLabel: t('ws_confirm'),
        cancelLabel: t('ws_cancel'),
      }))
    )
      return;
    await act(
      action === 'end'
        ? { action: 'end', id: item.id, discard: true }
        : { action: 'resolve', id: item.id, choice: action, destination: null },
    );
  }
  async function closeSaveAs() {
    if (!saveAs || inFlight.current || closingDraft.current) return;
    const draft = saveAs;
    closingDraft.current = true;
    try {
      if (
        draft.path !== draft.initialPath &&
        !(await confirm({
          title: t('ws_discard_title'),
          description: t('ui_discard_form_body'),
          tone: 'warning',
          confirmLabel: t('ws_confirm'),
          cancelLabel: t('ws_cancel'),
        }))
      )
        return;
      setSaveAs((current) => (current === draft ? null : current));
      setError(null);
    } finally {
      closingDraft.current = false;
    }
  }
  async function submitSaveAs() {
    if (!saveAs || !saveAs.path.startsWith('/') || saveAs.path === saveAs.initialPath) return;
    const draft = saveAs;
    const result = await act({
      action: 'resolve',
      id: draft.id,
      choice: 'saveAs',
      destination: draft.path,
    });
    if (result?.ok) setSaveAs((current) => (current === draft ? null : current));
  }
  return (
    <>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent closeLabel={t('ws_close')} className="flex max-h-[85vh] flex-col">
          <DialogHeader>
            <DialogTitle>{t('file_editors_title')}</DialogTitle>
          </DialogHeader>
          {(error || readError) && !saveAs ? (
            <Alert>
              <AlertDescription>
                {t(userErrorKey(error ?? readError ?? 'runtime_unavailable'))}
                {readError ? (
                  <Button variant="link" size="xs" onClick={() => void editingStore.refresh()}>
                    {t('retry')}
                  </Button>
                ) : null}
              </AlertDescription>
            </Alert>
          ) : null}
          <div className="min-h-0 overflow-auto">
            {!items.length ? (
              <Empty>
                <EmptyHeader>
                  <EmptyDescription>{t('file_editor_empty')}</EmptyDescription>
                </EmptyHeader>
              </Empty>
            ) : null}
            <div className="flex flex-col gap-4">
              {[...items]
                .sort(
                  (a, b) =>
                    Number(b.id === notificationTarget?.id) -
                    Number(a.id === notificationTarget?.id),
                )
                .map((item) => {
                  const session = state.sessions.find(
                    (s) => s.connectionId === item.connectionId && s.phase === 'running',
                  );
                  const inactive =
                    ['disconnected'].includes(item.phase) || (item.phase === 'unknown' && !session);
                  return (
                    <EditorNoticeSurface key={item.id} id={item.id}>
                      <TruncatedText>{item.path}</TruncatedText>
                      <FieldLabel>{t(`file_editor_${item.phase}` as MessageKey)}</FieldLabel>
                      {item.error ? (
                        <Alert>
                          <AlertDescription>{t(userErrorKey(item.error))}</AlertDescription>
                        </Alert>
                      ) : null}
                      <div className="flex flex-wrap gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={busy}
                          onClick={() => void act({ action: 'reopen', id: item.id })}
                        >
                          {t('file_editor_reopen')}
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={busy}
                          onClick={() => void act({ action: 'export', id: item.id })}
                        >
                          {t('file_editor_export')}
                        </Button>
                        {inactive ? (
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={busy || !session}
                            onClick={() =>
                              session &&
                              void act({ action: 'recover', id: item.id, sessionId: session.id })
                            }
                          >
                            {t('file_editor_recover')}
                          </Button>
                        ) : null}
                        {!inactive && ['failed', 'unknown', 'pending'].includes(item.phase) ? (
                          <Button
                            variant="outline"
                            size="sm"
                            disabled={busy}
                            onClick={() => void act({ action: 'retry', id: item.id })}
                          >
                            {t('retry')}
                          </Button>
                        ) : null}
                        {item.phase === 'conflict' ? (
                          <>
                            <Button
                              variant="outline"
                              size="sm"
                              disabled={busy}
                              onClick={() => {
                                setError(null);
                                setSaveAs({ id: item.id, path: item.path, initialPath: item.path });
                              }}
                            >
                              {t('file_editor_save_as')}
                            </Button>
                            <Button
                              variant="outline"
                              size="sm"
                              disabled={busy}
                              onClick={() => void destructive(item, 'remote')}
                            >
                              {t('file_editor_use_remote')}
                            </Button>
                            <Button
                              variant="outline"
                              size="sm"
                              disabled={busy}
                              onClick={() => void destructive(item, 'overwrite')}
                            >
                              {t('file_editor_overwrite')}
                            </Button>
                          </>
                        ) : null}
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={busy || item.phase === 'uploading' || item.phase === 'unknown'}
                          onClick={async () => {
                            const result = await act(
                              { action: 'end', id: item.id, discard: false },
                              'file_editor_unsynced',
                            );
                            if (result?.error === 'file_editor_unsynced')
                              await destructive(item, 'end');
                          }}
                        >
                          {t('file_editor_end')}
                        </Button>
                      </div>
                      {inactive && !session ? <p>{t('file_editor_recovery_hint')}</p> : null}
                    </EditorNoticeSurface>
                  );
                })}
            </div>
          </div>
        </DialogContent>
      </Dialog>
      <FormDialog
        open={!!saveAs}
        onOpenChange={(value) => {
          if (!value) void closeSaveAs();
        }}
      >
        <DialogContent closeLabel={t('ws_close')}>
          <DialogHeader>
            <DialogTitle>{t('file_editor_save_as')}</DialogTitle>
          </DialogHeader>
          <DialogForm
            onSubmit={(event) => {
              event.preventDefault();
              void submitSaveAs();
            }}
          >
            <Field data-invalid={!!error}>
              <FieldLabel htmlFor="file-editor-save-as">{t('tools_path')}</FieldLabel>
              <Input
                id="file-editor-save-as"
                value={saveAs?.path ?? ''}
                disabled={busy}
                aria-invalid={!!error}
                aria-describedby={error ? 'file-editor-save-as-error' : undefined}
                onChange={(event) => {
                  setError(null);
                  setSaveAs((draft) => draft && { ...draft, path: event.target.value });
                }}
              />
              <FieldError id="file-editor-save-as-error">
                {error ? t(userErrorKey(error)) : undefined}
              </FieldError>
            </Field>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={busy}
                onClick={() => void closeSaveAs()}
              >
                {t('ws_cancel')}
              </Button>
              <Button
                type="submit"
                disabled={
                  busy || !saveAs?.path.startsWith('/') || saveAs.path === saveAs.initialPath
                }
              >
                {t('ws_confirm')}
              </Button>
            </DialogFooter>
          </DialogForm>
        </DialogContent>
      </FormDialog>
    </>
  );
}

function EditorNoticeSurface({ id, children }: { id: string; children: React.ReactNode }) {
  const ref = useNotificationSurface(`fileEditor:${id}`);
  return (
    <div ref={ref} className="flex min-w-0 flex-col gap-2">
      {children}
    </div>
  );
}
