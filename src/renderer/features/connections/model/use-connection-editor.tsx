import type { EditorSection } from './form-validation';
import { credentialEditorState } from '../credential-editor-state';

import { useConfirmation } from '../../../shared/ConfirmationProvider';

import { operationCanCancel, operationBusy, operationRunning } from '../operations';

import type { MessageKey } from '../../../../shared/i18n/messages';

import { useEffect, useRef, useState, useCallback } from 'react';

import { useForm, useController, type FieldErrors } from 'react-hook-form';

import { zodResolver } from '@hookform/resolvers/zod';

import type {
  ConnectionConfig,
  ConnectionDetails,
  CredentialDraftView,
  WorkspaceReply,
} from '@contracts';

import { useDesktop } from '../../../app/DesktopProvider';

import { Button } from '../../../components/ui/button';

import { Alert, AlertTitle, AlertDescription } from '../../../components/ui/alert';

import { useWorkspace, errorCode } from '../workspace-context';

import { useWorkspaceUi } from '../ui-context';

import { WorkspaceFailure, type Editor } from '../workspace-store';

import { connectionFormValues, type ConnectionFormValues } from './form-validation';

import { useConnectionSchema } from './use-connection-schema';

export function useConnectionEditor({
  editor,
}: {
  editor: Extract<Editor, { kind: 'connection' }>;
}) {
  const { store, state } = useWorkspace();
  const { t } = useDesktop();
  const confirm = useConfirmation();
  const ui = useWorkspaceUi();
  const schema = useConnectionSchema();
  const [section, setSection] = useState<EditorSection>('connection');
  const form = useForm<ConnectionFormValues>({
    resolver: zodResolver(schema),
    defaultValues: editor.connection
      ? connectionFormValues(editor.connection.config)
      : {
          notes: '',
          terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
          proxyId: null,
          tunnels: [],
          name: '',
          host: '',
          port: 22,
          username: '',
          auth: 'password',
          environment: 'none',
        },
  });
  const { field: portField } = useController({
    name: 'port',
    control: form.control,
  });
  const [portText, setPortText] = useState(() => {
    const port = editor.connection?.config.port ?? 22;
    return port === 22 ? '' : String(port);
  });
  const [base, setBase] = useState(editor.connection);
  const [draft, setDraft] = useState<CredentialDraftView | null>(null),
    [activeAction, setActiveAction] = useState<MessageKey | null>(null),
    [error, setError] = useState<string | null>(null),
    [conflict, setConflict] = useState<ConnectionDetails | null>(null),
    [tested, setTested] = useState<{ id: string; config: string } | null>(null),
    [unknown, setUnknown] = useState<WorkspaceFailure | null>(null);
  const owner = 'editor:' + editor.token;
  const childClose = useRef<(() => Promise<void>) | null>(null);
  const actionLock = useRef<MessageKey | null>(null);
  const resources = base ? ['connection:' + base.id] : ['navigation:connections'];
  const credentialBlocked = store.feedback.isBusy('credential-draft');
  const busy = activeAction !== null || store.feedback.isBusy(owner, ...resources);
  function begin(action: MessageKey) {
    if (actionLock.current || store.feedback.isBusy(owner, ...resources)) return false;
    actionLock.current = action;
    setActiveAction(action);
    return true;
  }
  const end = useCallback(() => {
    actionLock.current = null;
    setActiveAction(null);
  }, []);
  const setBusy = useCallback(
    (value: boolean) => {
      if (value) {
        actionLock.current = 'ws_close';
        setActiveAction('ws_close');
      } else end();
    },
    [end],
  );
  const runtime = useRef(state.runtimeId),
    mounted = useRef(true),
    draftRef = useRef(draft);
  draftRef.current = draft;
  const auth = form.watch('auth'),
    config = form.watch();
  const currentTestConfig = tested ? schema.safeParse(config) : null;
  const passedTestId =
    tested && currentTestConfig?.success && JSON.stringify(currentTestConfig.data) === tested.config
      ? tested.id
      : null;
  const credentialState = credentialEditorState(auth, base, draft);
  const dirty = form.formState.isDirty || credentialState.dirty;
  const credentialReady = credentialState.ready;
  const draftPending = useRef<Promise<CredentialDraftView> | null>(null),
    credentialButton = useRef<HTMLButtonElement | null>(null);
  const ensureDraft = async () => {
    if (draftRef.current) return draftRef.current;
    if (draftPending.current) return draftPending.current;
    const epoch = state.runtimeId;
    const pending = store.call('beginDraft', { connectionId: base?.id ?? null }).then((reply) => {
      if (reply.kind !== 'draft') throw new WorkspaceFailure('protocol_mismatch');
      if (!mounted.current || store.getSnapshot().runtimeId !== epoch) {
        void store.call('discardDraft', { draftId: reply.draft.id }).catch(() => undefined);
        throw new WorkspaceFailure('runtime_unavailable');
      }
      draftRef.current = reply.draft;
      setDraft(reply.draft);
      return reply.draft;
    });
    draftPending.current = pending;
    try {
      return await pending;
    } finally {
      if (draftPending.current === pending) draftPending.current = null;
    }
  };
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (draftRef.current && !store.feedback.isBusy(owner))
        void store.call('discardDraft', { draftId: draftRef.current.id }).catch(() => undefined);
    };
  }, [store, owner]);
  const { setDirty } = ui;
  useEffect(() => {
    setDirty(dirty);
  }, [dirty, setDirty]);
  useEffect(() => {
    if (runtime.current !== state.runtimeId) {
      runtime.current = state.runtimeId;
      const lostDraft = !!draftRef.current;
      draftRef.current = null;
      setDraft(null);
      setTested(null);
      setBusy(false);
      if (lostDraft) setError('draft_lost');
    }
  }, [state.runtimeId, setBusy]);
  const run = (action: MessageKey, execute: () => Promise<WorkspaceReply>) =>
    store.run({
      owner,
      resources: [
        ...resources,
        ...(draftRef.current ||
        ['ws_set_password', 'ws_choose_key', 'ws_test', 'ws_clear_credential'].includes(action)
          ? ['credential-draft']
          : []),
      ],
      action,
      label: form.getValues('name') || t('ws_new_connection'),
      execute,
      onResult: async (reply) => {
        if (reply.kind === 'draft') {
          if (!mounted.current || store.getSnapshot().editor?.token !== editor.token) {
            await store.call('discardDraft', { draftId: reply.draft.id }).catch(() => undefined);
            draftRef.current = null;
            return;
          }
          draftRef.current = reply.draft;
          setDraft(reply.draft);
          setTested(null);
          return;
        }
        if (reply.kind !== 'operation') return;
        if (!mounted.current || store.getSnapshot().editor?.token !== editor.token) {
          const draftId = reply.operation.draft?.id ?? draftRef.current?.id;
          if (draftId) await store.call('discardDraft', { draftId }).catch(() => undefined);
          draftRef.current = null;
          return;
        }
        const job = reply.operation;
        setUnknown(null);
        if (job.connection) {
          setBase(job.connection);
          draftRef.current = null;
          setDraft(null);
          form.reset(connectionFormValues(job.connection.config));
          setPortText(job.connection.config.port === 22 ? '' : String(job.connection.config.port));
        }
        if (job.draft) {
          draftRef.current = job.draft;
          setDraft(job.draft);
        }
      },
    });
  async function close() {
    if (childClose.current) {
      await childClose.current();
      return;
    }
    const operation = store.feedback.forOwner(owner);
    if (operation && operationBusy(operation)) {
      if (
        !['ws_save', 'ws_save_connect'].includes(operation.action) &&
        dirty &&
        !(await confirm({
          tone: 'warning',
          title: t('ws_discard_title'),
          description: t('ws_discard_body'),
          confirmLabel: t('ws_confirm'),
          cancelLabel: t('ws_cancel'),
        }))
      )
        return;
      if (!operationRunning(operation) || !operationCanCancel(operation)) {
        store.closeEditor();
        return;
      }
      await store.feedback.cancel(operation.id);
      if (store.feedback.forOwner(owner)?.cancelError) return;
      try {
        await store.waitOperation(operation.job!.id);
      } catch {
        return;
      }
    } else if (
      dirty &&
      !(await confirm({
        tone: 'warning',
        title: t('ws_discard_title'),
        description: t('ws_discard_body'),
        confirmLabel: t('ws_confirm'),
        cancelLabel: t('ws_cancel'),
      }))
    )
      return;
    setBusy(true);
    try {
      if (draftRef.current) await store.call('discardDraft', { draftId: draftRef.current.id });
      store.closeEditor();
    } catch (e) {
      setError(errorCode(e));
      setBusy(false);
    }
  }
  useEffect(() => {
    ui.setCloseEditor(close);
    return () => ui.setCloseEditor(null);
  });
  async function clearCredential() {
    if (!credentialReady || busy || credentialBlocked || !state.ready) return;
    if (
      !(await confirm({
        tone: 'danger',
        title: t(
          !!base?.credentialSaved && base.config.auth === auth
            ? auth === 'password'
              ? 'ws_clear_saved_password_title'
              : 'ws_clear_saved_private_key_title'
            : auth === 'password'
              ? 'ws_clear_password_title'
              : 'ws_clear_private_key_title',
        ),
        description: t(
          !!base?.credentialSaved && base.config.auth === auth
            ? auth === 'password'
              ? 'ws_clear_saved_password_body'
              : 'ws_clear_saved_private_key_body'
            : auth === 'password'
              ? 'ws_clear_password_body'
              : 'ws_clear_private_key_body',
        ),
        confirmLabel: t('ws_confirm'),
        cancelLabel: t('ws_cancel'),
      }))
    )
      return;
    if (!begin('ws_clear_credential')) return;
    setError(null);
    try {
      await run('ws_clear_credential', async () => {
        const d = await ensureDraft();
        return store.call('clearDraft', { draftId: d.id });
      });
    } catch (e) {
      if (mounted.current) setError(errorCode(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  async function configure() {
    const action = auth === 'password' ? 'ws_set_password' : 'ws_choose_key';
    if (!begin(action)) return;
    setError(null);
    setTested(null);
    try {
      const reply = await run(action, async () => {
        const d = await ensureDraft();
        return store.mutation('configureDraft', { draftId: d.id, auth });
      });
      if (reply.kind !== 'operation') throw new WorkspaceFailure('protocol_mismatch');
      const result = await store.settled(reply);
      if (result?.draft) {
        setDraft(result.draft);
        draftRef.current = result.draft;
      }
    } catch (e) {
      if (errorCode(e) !== 'cancelled') setError(errorCode(e));
    } finally {
      setBusy(false);
      requestAnimationFrame(() => {
        if (credentialButton.current?.checkVisibility()) credentialButton.current.focus();
      });
    }
  }
  async function test(values: ConnectionConfig) {
    if (!begin('ws_test')) return;
    setError(null);
    setTested(null);
    try {
      const reply = await run('ws_test', async () => {
        if (!credentialReady) {
          setSection('connection');
          throw new WorkspaceFailure('credential_missing');
        }
        const d = await ensureDraft();
        return store.mutation('testConnection', {
          connectionId: base?.id ?? null,
          config: values,
          draftId: d.id,
        });
      });
      if (reply.kind !== 'operation') throw new WorkspaceFailure('protocol_mismatch');
      const result = await store.settled(reply);
      const current = store.getSnapshot();
      if (
        result &&
        mounted.current &&
        current.editor?.token === editor.token &&
        current.runtimeId === state.runtimeId
      )
        setTested({ id: result.id, config: JSON.stringify(values) });
    } catch (e) {
      const code = errorCode(e);
      const failure = store.feedback.forOwner(owner);
      if (code !== 'cancelled' && !(failure?.action === 'ws_test' && failure.error?.code === code))
        setError(code);
    } finally {
      setBusy(false);
    }
  }
  async function acceptSave(reply: WorkspaceReply, connect: boolean) {
    if (reply.kind !== 'operation') throw new WorkspaceFailure('protocol_mismatch');
    const result = await store.settled(reply);
    if (!result?.connection) throw new WorkspaceFailure('protocol_mismatch');
    const c = result.connection;
    draftRef.current = null;
    const isCurrent = store.getSnapshot().editor?.token === editor.token;
    if (isCurrent) store.closeEditor();
    await store.refresh();
    if (isCurrent) await store.select({ kind: 'connection', id: c.id });
    store.report('saved');
    if (connect) await store.connect(c.id);
  }
  async function save(values: ConnectionConfig, connect: boolean) {
    if (connect && !credentialReady) {
      setSection('connection');
      setError('credential_missing');
      return;
    }
    if (!begin(connect ? 'ws_save_connect' : 'ws_save')) return;
    setError(null);
    setConflict(null);
    try {
      const request = {
        connectionId: base?.id ?? null,
        expectedVersion: base?.version ?? null,
        groupId: base ? null : editor.groupId,
        config: values,
        draftId: draftRef.current?.id ?? null,
      };
      const reply = await run(connect ? 'ws_save_connect' : 'ws_save', () =>
        store.mutation(base ? 'updateConnection' : 'createConnection', request),
      );
      await acceptSave(reply, connect);
    } catch (e) {
      if (errorCode(e) !== 'cancelled') setError(errorCode(e));
      if (e instanceof WorkspaceFailure && e.code === 'result_unknown') setUnknown(e);
      if (e instanceof WorkspaceFailure && e.code === 'conflict' && base) {
        const current = await store.call('getConnection', { id: base.id });
        if (current.kind === 'connection') setConflict(current.connection);
      }
    } finally {
      setBusy(false);
    }
  }
  async function resolveConflict(load: boolean) {
    if (!conflict) return;
    const removalPending = !load && draftRef.current?.intent === 'remove';
    if (draftRef.current) await store.call('discardDraft', { draftId: draftRef.current.id });
    draftRef.current = null;
    setDraft(null);
    if (removalPending) {
      const reply = await store.call('beginDraft', {
        connectionId: conflict.id,
      });
      if (reply.kind !== 'draft') throw new WorkspaceFailure('protocol_mismatch');
      draftRef.current = reply.draft;
      setDraft(reply.draft);
      const cleared = await store.call('clearDraft', {
        draftId: reply.draft.id,
      });
      if (cleared.kind !== 'draft') throw new WorkspaceFailure('protocol_mismatch');
      draftRef.current = cleared.draft;
      setDraft(cleared.draft);
    }
    setBase(conflict);
    if (load) {
      form.reset(connectionFormValues(conflict.config));
      setPortText(conflict.config.port === 22 ? '' : String(conflict.config.port));
    }
    setConflict(null);
    setError(null);
    setTested(null);
  }
  const conflictDetails = conflict ? (
    <Alert>
      <AlertTitle>{t('ws_conflict_title')}</AlertTitle>
      <AlertDescription>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <strong>{t('ws_current_values')}</strong>
            <pre className="whitespace-pre-wrap break-all">
              {[
                conflict.config.name,
                conflict.config.host,
                String(conflict.config.port),
                conflict.config.username,
                t(conflict.config.auth === 'privateKey' ? 'ws_private_key' : 'ws_password'),
              ].join('\n')}
            </pre>
          </div>
          <div>
            <strong>{t('ws_your_values')}</strong>
            <pre className="whitespace-pre-wrap break-all">
              {[
                config.name,
                config.host,
                String(config.port),
                config.username,
                t(config.auth === 'privateKey' ? 'ws_private_key' : 'ws_password'),
              ].join('\n')}
            </pre>
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button type="button" variant="outline" onClick={() => void resolveConflict(true)}>
            {t('ws_load_current')}
          </Button>
          <Button type="button" variant="outline" onClick={() => void resolveConflict(false)}>
            {t('ws_keep_draft')}
          </Button>
        </div>
      </AlertDescription>
    </Alert>
  ) : null;
  const errors = form.formState.errors;
  function showInvalidFields(errors: FieldErrors<ConnectionFormValues>) {
    const field = (['name', 'host', 'port', 'username'] as const).find((key) => errors[key]);
    if (!field) {
      if (errors.terminal) {
        setSection('terminal');
        const field = errors.terminal.encoding
          ? 'encoding'
          : errors.terminal.backspace
            ? 'backspace'
            : 'delete';
        requestAnimationFrame(() => document.getElementById('connection-' + field)?.focus());
      } else if (errors.proxyId) {
        setSection('proxy');
        requestAnimationFrame(() => document.getElementById('connection-proxy')?.focus());
      } else if (errors.tunnels) setSection('tunnels');
      else if (errors.notes) {
        setSection('connection');
        requestAnimationFrame(() => form.setFocus('notes'));
      }
      return;
    }
    setSection('connection');
    requestAnimationFrame(() => form.setFocus(field));
  }
  return {
    state,
    t,
    ui,
    section,
    setSection,
    form,
    portField,
    portText,
    setPortText,
    base,
    draft,
    error,
    passedTestId,
    clearTestResult: () => setTested(null),
    unknown,
    owner,
    childClose,
    credentialBlocked,
    busy,
    auth,
    config,
    credentialState,
    credentialReady,
    credentialButton,
    close,
    clearCredential,
    configure,
    test,
    save,
    conflictDetails,
    errors,
    showInvalidFields,
  };
}
