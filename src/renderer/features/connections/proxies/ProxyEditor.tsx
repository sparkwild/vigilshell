import { CredentialStorageNotice } from '../CredentialStorageNotice';
import { useConfirmation } from '../../../shared/ConfirmationProvider';

import { useEffect, useRef, useState } from 'react';

import type { CredentialDraftView, ProxyConfig, ProxyView } from '@contracts';

import { useDesktop } from '../../../app/DesktopProvider';

import { Button } from '../../../components/ui/button';

import { Input } from '../../../components/ui/input';

import { Checkbox } from '../../../components/ui/checkbox';

import {
  Field,
  FieldLabel,
  FieldDescription,
  FieldGroup,
  FieldError,
} from '../../../components/ui/field';

import {
  FormDialog,
  DialogForm,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../../../components/ui/dialog';

import { proxyConfigSchema } from '../model/form-validation';

import { useWorkspace, errorCode, errorKey } from '../workspace-context';

import { OperationStatus, OperationButtonIcon } from '../OperationFeedback';

import { operationCanCancel, operationRunning } from '../operations';

import { WorkspaceFailure } from '../workspace-store';

import { OptionSelect } from '../components/OptionSelect';

export function ProxyEditor({
  proxy,
  onClose,
  onSaved,
  credentialBlocked,
  onChildClose,
}: {
  proxy: ProxyView | null;
  onClose(): void;
  onSaved(proxy: ProxyView): void;
  credentialBlocked: boolean;
  onChildClose(close: (() => Promise<void>) | null): void;
}) {
  const { store, state } = useWorkspace(),
    confirm = useConfirmation(),
    { t } = useDesktop();
  const [config, setConfig] = useState<ProxyConfig>(
    proxy?.config ?? {
      name: '',
      kind: 'socks5',
      host: '',
      port: 0,
      username: '',
    },
  );
  const [draft, setDraft] = useState<CredentialDraftView | null>(null),
    [busy, setBusy] = useState(false),
    [clear, setClear] = useState(false),
    [error, setError] = useState<string | null>(null);
  const [validationAttempted, setValidationAttempted] = useState(false);
  const validation = validationAttempted ? proxyConfigSchema.safeParse(config) : null;
  const invalidFields = new Set(
    validation && !validation.success
      ? validation.error.issues.map((issue) => String(issue.path[0]))
      : [],
  );
  const [owner] = useState(() => 'proxy-editor:' + crypto.randomUUID()),
    alive = useRef(true),
    draftRef = useRef(draft),
    lock = useRef(false);
  draftRef.current = draft;
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      if (draftRef.current && !store.feedback.isBusy(owner))
        void store.call('discardDraft', { draftId: draftRef.current.id }).catch(() => undefined);
    };
  }, [owner, store]);
  const dirty =
    JSON.stringify(config) !==
      JSON.stringify(
        proxy?.config ?? {
          name: '',
          kind: 'socks5',
          host: '',
          port: 0,
          username: '',
        },
      ) ||
    !!draft?.prepared ||
    clear;
  async function close() {
    const op = store.feedback.forOwner(owner);
    if (op && operationRunning(op) && operationCanCancel(op)) {
      await store.feedback.cancel(op.id);
      return;
    }
    if (op && store.feedback.isBusy(owner)) {
      onClose();
      return;
    }
    if (
      dirty &&
      !(await confirm({
        tone: 'warning',
        title: t('ws_discard_title'),
        description: t('ws_discard_proxy_body'),
        confirmLabel: t('ws_confirm'),
        cancelLabel: t('ws_cancel'),
      }))
    )
      return;
    if (draft) await store.call('discardDraft', { draftId: draft.id }).catch(() => undefined);
    onClose();
  }
  useEffect(() => {
    onChildClose(close);
    return () => onChildClose(null);
  });
  async function configure() {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError(null);
    try {
      let d = draft;
      if (!d) {
        const r = await store.call('beginProxyDraft', {
          proxyId: proxy?.id ?? null,
        });
        if (r.kind !== 'draft') throw new WorkspaceFailure('protocol_mismatch');
        d = r.draft;
        if (!alive.current) {
          await store.call('discardDraft', { draftId: d.id }).catch(() => undefined);
          return;
        }
        setDraft(d);
        draftRef.current = d;
      }
      const r = await store.run({
        owner,
        action: 'ws_set_password',
        label: config.name || t('ws_proxy'),
        resources: ['credential-draft'],
        execute: () => store.mutation('configureDraft', { draftId: d.id, auth: 'password' }),
      });
      if (r.kind === 'operation' && r.operation.draft && alive.current) {
        setDraft(r.operation.draft);
        setClear(false);
      }
    } catch (e) {
      if (errorCode(e) !== 'cancelled') setError(errorCode(e));
    } finally {
      lock.current = false;
      if (alive.current) setBusy(false);
    }
  }
  async function save() {
    if (lock.current || store.feedback.isBusy(owner)) return;
    setValidationAttempted(true);
    const parsed = proxyConfigSchema.safeParse(config);
    if (!parsed.success) {
      const field = String(parsed.error.issues[0]?.path[0] ?? 'name');
      document.getElementById('proxy-' + (field === 'username' ? 'user' : field))?.focus();
      return;
    }
    lock.current = true;
    setBusy(true);
    setError(null);
    try {
      const reply = await store.run({
        owner,
        action: 'ws_save_proxy',
        label: config.name,
        resources: proxy ? ['proxy:' + proxy.id] : [],
        execute: () =>
          store.mutation('saveProxy', {
            proxyId: proxy?.id ?? null,
            expectedVersion: proxy?.version ?? null,
            config: parsed.data,
            draftId: draft?.id ?? null,
            clearPassword: clear,
          }),
        onResult: (r) => {
          if (r.kind === 'operation' && r.operation.proxy && alive.current) {
            draftRef.current = null;
            onSaved(r.operation.proxy);
          }
        },
      });
      if (reply.kind !== 'operation') throw new WorkspaceFailure('protocol_mismatch');
    } catch (e) {
      setError(errorCode(e));
    } finally {
      lock.current = false;
      if (alive.current) setBusy(false);
    }
  }
  return (
    <FormDialog
      open
      onOpenChange={(open) => {
        if (!open) void close();
      }}
    >
      <DialogContent size="form" closeLabel={t('ws_close')}>
        <DialogHeader>
          <DialogTitle>{t(proxy ? 'ws_edit_proxy' : 'ws_new_proxy')}</DialogTitle>
          <DialogDescription>{t('ws_proxy_saved_independently')}</DialogDescription>
        </DialogHeader>
        <DialogForm
          onSubmit={(e) => {
            e.preventDefault();
            e.stopPropagation();
            void save();
          }}
        >
          <FieldGroup density="tight">
            <Field data-invalid={invalidFields.has('name')}>
              <FieldLabel htmlFor="proxy-name">{t('ws_option_name')}</FieldLabel>
              <Input
                id="proxy-name"
                aria-invalid={invalidFields.has('name')}
                aria-describedby={invalidFields.has('name') ? 'proxy-name-error' : undefined}
                placeholder={t('ws_hint_proxy_name')}
                value={config.name}
                onChange={(e) => setConfig({ ...config, name: e.target.value })}
                disabled={busy}
                autoFocus
              />
              <FieldError id="proxy-name-error">
                {invalidFields.has('name') ? t('ui_field_invalid') : undefined}
              </FieldError>
            </Field>
            <OptionSelect
              id="proxy-kind"
              label={t('ws_type')}
              value={config.kind}
              options={[
                { value: 'socks5', label: 'SOCKS5' },
                { value: 'http', label: 'HTTP CONNECT' },
              ]}
              disabled={busy}
              onChange={(kind) => setConfig({ ...config, kind: kind as ProxyConfig['kind'] })}
            />
            <div className="connection-editor-endpoint">
              <Field data-invalid={invalidFields.has('host')}>
                <FieldLabel htmlFor="proxy-host">{t('ws_host')}</FieldLabel>
                <Input
                  id="proxy-host"
                  aria-invalid={invalidFields.has('host')}
                  aria-describedby={invalidFields.has('host') ? 'proxy-host-error' : undefined}
                  placeholder={t('ws_hint_proxy_host')}
                  value={config.host}
                  onChange={(e) => setConfig({ ...config, host: e.target.value })}
                  disabled={busy}
                />
                <FieldError id="proxy-host-error">
                  {invalidFields.has('host') ? t('ui_field_invalid') : undefined}
                </FieldError>
              </Field>
              <Field data-invalid={invalidFields.has('port')}>
                <FieldLabel htmlFor="proxy-port">{t('ws_port')}</FieldLabel>
                <Input
                  id="proxy-port"
                  aria-invalid={invalidFields.has('port')}
                  aria-describedby={invalidFields.has('port') ? 'proxy-port-error' : undefined}
                  placeholder={config.kind === 'socks5' ? '1080' : '8080'}
                  type="number"
                  min={1}
                  max={65535}
                  value={config.port || ''}
                  onChange={(e) => setConfig({ ...config, port: Number(e.target.value) })}
                  disabled={busy}
                />
                <FieldError id="proxy-port-error">
                  {invalidFields.has('port') ? t('ui_field_invalid') : undefined}
                </FieldError>
              </Field>
            </div>
            <Field data-invalid={invalidFields.has('username')}>
              <FieldLabel htmlFor="proxy-user">{t('ws_username')}</FieldLabel>
              <Input
                id="proxy-user"
                aria-invalid={invalidFields.has('username')}
                aria-describedby={invalidFields.has('username') ? 'proxy-user-error' : undefined}
                placeholder={t('ws_hint_proxy_username')}
                value={config.username}
                onChange={(e) => setConfig({ ...config, username: e.target.value })}
                disabled={busy}
              />
              <FieldError id="proxy-user-error">
                {invalidFields.has('username') ? t('ui_field_invalid') : undefined}
              </FieldError>
            </Field>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                type="button"
                variant="outline"
                onClick={() => void configure()}
                disabled={busy || credentialBlocked || !state.transportReady}
              >
                <OperationButtonIcon owner={owner} action="ws_set_password">
                  {null}
                </OperationButtonIcon>
                {t('ws_set_password')}
              </Button>
              <span className="text-xs text-muted-foreground">
                {t(
                  draft?.prepared
                    ? 'ws_credential_prepared'
                    : proxy?.credentialSaved && !clear
                      ? 'ws_credential_saved'
                      : 'ws_credential_missing',
                )}
              </span>
            </div>
            <FieldDescription className="m-0 nth-last-2:mt-0">
              {t('ws_credential_storage_notice')} <CredentialStorageNotice />
            </FieldDescription>
            {credentialBlocked && !busy ? (
              <FieldDescription>{t('ws_proxy_credential_busy')}</FieldDescription>
            ) : null}
            {proxy?.credentialSaved ? (
              <Field orientation="horizontal">
                <Checkbox
                  id="proxy-clear"
                  checked={clear}
                  disabled={busy || !!draft?.prepared}
                  onCheckedChange={setClear}
                />
                <FieldLabel htmlFor="proxy-clear">{t('ws_clear_proxy_password')}</FieldLabel>
              </Field>
            ) : null}
          </FieldGroup>
          <OperationStatus owner={owner} message={error ? t(errorKey(error)) : null} />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => void close()}>
              {t(busy ? 'ws_close' : 'ws_cancel')}
            </Button>
            <Button
              type="submit"
              disabled={busy || store.feedback.isBusy(owner) || !state.transportReady}
            >
              {t('ws_save')}
            </Button>
          </DialogFooter>
        </DialogForm>
      </DialogContent>
    </FormDialog>
  );
}
