import { CredentialStorageNotice } from './CredentialStorageNotice';
import type { EditorSection } from './model/form-validation';
import { TruncatedText } from '../../components/ui/tooltip';

import { DefaultValueHint } from './DefaultValueHint';

import { OperationStatus, OperationButtonIcon } from './OperationFeedback';

import { environments, EnvironmentBadge } from './EnvironmentBadge';

import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectGroup,
  SelectItem,
} from '../../components/ui/select';

import {
  CheckIcon,
  EraserIcon,
  KeyRoundIcon,
  LockKeyholeIcon,
  SaveIcon,
  PlayIcon,
  FlaskConicalIcon,
  ServerIcon,
  NetworkIcon,
  ArrowLeftRightIcon,
  TerminalIcon,
} from 'lucide-react';

import { Button } from '../../components/ui/button';

import { Input } from '../../components/ui/input';

import { Badge } from '../../components/ui/badge';

import { ToggleGroup, ToggleGroupItem } from '../../components/ui/toggle-group';

import {
  Field,
  FieldLabel,
  FieldGroup,
  FieldSet,
  FieldLegend,
  FieldError,
  FieldDescription,
} from '../../components/ui/field';

import {
  FormDialog,
  DialogForm,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../../components/ui/dialog';

import { errorKey } from './workspace-context';

import { type Editor } from './workspace-store';

import { Tabs, TabsList, TabsTrigger, TabsContent } from '../../components/ui/tabs';

import { Textarea } from '../../components/ui/textarea';

import { TerminalOptions, ProxyOptions, TunnelOptions } from './connection-options';

import { type ConnectionFormValues } from './model/form-validation';

import { useConnectionEditor } from './model/use-connection-editor';

export function ConnectionEditor({ editor }: { editor: Extract<Editor, { kind: 'connection' }> }) {
  const model = useConnectionEditor({ editor });
  const {
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
    clearTestResult,
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
  } = model;

  return (
    <FormDialog
      mode="workspace"
      open={ui.active}
      onOpenChange={(open) => {
        if (!open) void close();
      }}
    >
      <DialogContent size="editor" closeLabel={t('ws_close')}>
        <DialogHeader>
          <DialogTitle>{t(base ? 'ws_edit_connection' : 'ws_new_connection')}</DialogTitle>
          <DialogDescription>{t('ws_editor_description')}</DialogDescription>
        </DialogHeader>
        <DialogForm
          className="connection-form"
          onSubmit={form.handleSubmit((values) => save(values, false), showInvalidFields)}
          onKeyDownCapture={(e) => {
            if (e.key === 'Enter' && e.nativeEvent.isComposing) e.preventDefault();
          }}
        >
          <Tabs
            orientation="vertical"
            value={section}
            onValueChange={(value) => {
              if (['connection', 'terminal', 'proxy', 'tunnels'].includes(value))
                setSection(value as EditorSection);
            }}
            className="connection-editor-sections"
          >
            <TabsList aria-label={t('ws_editor_sections')} variant="editor">
              <TabsTrigger value="connection">
                <ServerIcon />
                {t('ws_editor_connection')}
              </TabsTrigger>
              <TabsTrigger value="terminal">
                <TerminalIcon />
                {t('ws_terminal_options')}
              </TabsTrigger>
              <TabsTrigger value="proxy">
                <NetworkIcon />
                {t('ws_proxy')}
              </TabsTrigger>
              <TabsTrigger value="tunnels">
                <ArrowLeftRightIcon />
                {t('ws_tunnels')}
              </TabsTrigger>
            </TabsList>
            <div className="connection-editor-content">
              <TabsContent value="connection" keepMounted className="connection-editor-panel">
                <FieldGroup density="compact">
                  <FieldSet disabled={busy}>
                    <div className="connection-editor-identity">
                      <Field className="connection-editor-field" data-invalid={!!errors.name}>
                        <div className="flex items-center gap-1">
                          <FieldLabel htmlFor="connection-name">
                            {t('ws_connection_name')}
                          </FieldLabel>
                          <DefaultValueHint label={t('ws_connection_name')}>
                            {t('ws_default_name_hint')}
                          </DefaultValueHint>
                        </div>
                        <Input
                          id="connection-name"
                          placeholder={config.host.trim() || t('ws_hint_connection_name')}
                          {...form.register('name')}
                          aria-invalid={!!errors.name}
                          aria-describedby={errors.name ? 'connection-name-error' : undefined}
                          autoFocus
                        />
                        <FieldError id="connection-name-error">{errors.name?.message}</FieldError>
                      </Field>
                      <Field>
                        <FieldLabel htmlFor="connection-environment">
                          {t('ws_environment')}
                        </FieldLabel>
                        <Select
                          value={config.environment}
                          onValueChange={(v) => {
                            if (v)
                              form.setValue(
                                'environment',
                                v as ConnectionFormValues['environment'],
                                { shouldDirty: true },
                              );
                          }}
                        >
                          <SelectTrigger id="connection-environment">
                            <SelectValue>
                              <EnvironmentBadge environment={config.environment} showNone />
                            </SelectValue>
                          </SelectTrigger>
                          <SelectContent>
                            <SelectGroup>
                              {environments.map((environment) => (
                                <SelectItem key={environment} value={environment}>
                                  <EnvironmentBadge environment={environment} showNone />
                                </SelectItem>
                              ))}
                            </SelectGroup>
                          </SelectContent>
                        </Select>
                      </Field>
                    </div>
                    <div className="connection-editor-endpoint">
                      <Field data-invalid={!!errors.host}>
                        <FieldLabel htmlFor="connection-host">{t('ws_host')}</FieldLabel>
                        <Input
                          id="connection-host"
                          placeholder={t('ws_hint_host')}
                          {...form.register('host')}
                          aria-invalid={!!errors.host}
                          aria-describedby={errors.host ? 'connection-host-error' : undefined}
                        />
                        <FieldError id="connection-host-error">{errors.host?.message}</FieldError>
                      </Field>
                      <Field data-invalid={!!errors.port}>
                        <div className="flex items-center gap-1">
                          <FieldLabel htmlFor="connection-port">{t('ws_port')}</FieldLabel>
                          <DefaultValueHint label={t('ws_port')}>
                            {t('ws_default_ssh_port_hint')}
                          </DefaultValueHint>
                        </div>
                        <Input
                          id="connection-port"
                          placeholder="22"
                          type="number"
                          min={1}
                          max={65535}
                          name={portField.name}
                          ref={portField.ref}
                          onBlur={portField.onBlur}
                          value={portText}
                          onChange={(event) => {
                            const value = event.target.value;
                            setPortText(value);
                            portField.onChange(value === '' ? 22 : Number(value));
                          }}
                          aria-invalid={!!errors.port}
                          aria-describedby={errors.port ? 'connection-port-error' : undefined}
                        />
                        <FieldError id="connection-port-error">{errors.port?.message}</FieldError>
                      </Field>
                    </div>
                    <Field className="connection-editor-field" data-invalid={!!errors.username}>
                      <FieldLabel htmlFor="connection-user">{t('ws_username')}</FieldLabel>
                      <Input
                        id="connection-user"
                        placeholder={t('ws_hint_username')}
                        {...form.register('username')}
                        aria-invalid={!!errors.username}
                        aria-describedby={errors.username ? 'connection-username-error' : undefined}
                        autoComplete="off"
                      />
                      <FieldError id="connection-username-error">
                        {errors.username?.message}
                      </FieldError>
                    </Field>
                  </FieldSet>
                  <FieldSet disabled={busy}>
                    <FieldLegend className="flex items-center gap-1">
                      {t('ws_authentication')}
                      <CredentialStorageNotice />
                    </FieldLegend>
                    <div className="connection-auth-row">
                      <ToggleGroup
                        appearance="segmented"
                        spacing={0}
                        multiple={false}
                        disabled={busy}
                        aria-label={t('ws_authentication')}
                        value={[auth]}
                        onKeyDown={(event) => {
                          if (
                            busy ||
                            event.altKey ||
                            event.ctrlKey ||
                            event.metaKey ||
                            !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)
                          )
                            return;
                          event.preventDefault();
                          const next =
                            event.key === 'Home'
                              ? 'password'
                              : event.key === 'End'
                                ? 'privateKey'
                                : auth === 'password'
                                  ? 'privateKey'
                                  : 'password';
                          form.setValue('auth', next, { shouldDirty: true });
                          clearTestResult();
                          event.currentTarget
                            .querySelector<HTMLButtonElement>(`[data-auth-method="${next}"]`)
                            ?.focus();
                        }}
                        onValueChange={(v) => {
                          if (v[0] === 'password' || v[0] === 'privateKey') {
                            form.setValue('auth', v[0], { shouldDirty: true });
                            clearTestResult();
                          }
                        }}
                      >
                        <ToggleGroupItem value="password" data-auth-method="password">
                          <LockKeyholeIcon data-icon="inline-start" />
                          {t('ws_password')}
                          <CheckIcon
                            className={
                              auth === 'password'
                                ? 'connection-auth-check'
                                : 'connection-auth-check invisible'
                            }
                            aria-hidden="true"
                          />
                        </ToggleGroupItem>
                        <ToggleGroupItem value="privateKey" data-auth-method="privateKey">
                          <KeyRoundIcon data-icon="inline-start" />
                          {t('ws_private_key')}
                          <CheckIcon
                            className={
                              auth === 'privateKey'
                                ? 'connection-auth-check'
                                : 'connection-auth-check invisible'
                            }
                            aria-hidden="true"
                          />
                        </ToggleGroupItem>
                      </ToggleGroup>
                      <div className="connection-auth-actions">
                        <Badge variant={credentialReady ? 'success' : 'warning'}>
                          {t(
                            credentialState.removalPending
                              ? 'ws_credential_removal_pending'
                              : credentialReady
                                ? 'ws_auth_configured'
                                : 'ws_auth_unconfigured',
                          )}
                        </Badge>
                        <Button
                          ref={credentialButton}
                          className="connection-auth-action"
                          disabled={credentialBlocked || !state.ready}
                          type="button"
                          variant="outline"
                          onClick={() => void configure()}
                        >
                          <OperationButtonIcon
                            owner={owner}
                            action={auth === 'password' ? 'ws_set_password' : 'ws_choose_key'}
                          >
                            {auth === 'password' ? <LockKeyholeIcon /> : <KeyRoundIcon />}
                          </OperationButtonIcon>
                          {t(
                            auth === 'password'
                              ? credentialReady
                                ? 'ws_change_password'
                                : 'ws_set_password'
                              : credentialReady
                                ? 'ws_change_key'
                                : 'ws_choose_key',
                          )}
                        </Button>
                        <span className="inline-flex has-[:disabled]:cursor-not-allowed">
                          <Button
                            type="button"
                            variant="destructive"
                            disabled={busy || credentialBlocked || !state.ready || !credentialReady}
                            onClick={() => void clearCredential()}
                            aria-label={t('ws_clear_auth')}
                          >
                            <EraserIcon data-icon="inline-start" aria-hidden="true" />
                            {t('ws_clear_auth')}
                          </Button>
                        </span>
                      </div>
                    </div>
                    {auth === 'privateKey' &&
                    credentialReady &&
                    (draft?.label || base?.credentialLabel) ? (
                      <FieldDescription className="min-w-0">
                        <TruncatedText className="block truncate">
                          {draft?.label ?? base?.credentialLabel ?? ''}
                        </TruncatedText>
                      </FieldDescription>
                    ) : null}
                  </FieldSet>
                  <Field data-invalid={!!errors.notes}>
                    <FieldLabel htmlFor="connection-notes">{t('ws_notes')}</FieldLabel>
                    <Textarea
                      id="connection-notes"
                      aria-invalid={!!errors.notes}
                      aria-describedby={errors.notes ? 'connection-notes-error' : undefined}
                      placeholder={t('ws_hint_notes')}
                      rows={3}
                      {...form.register('notes')}
                      disabled={busy}
                    />
                    <FieldError id="connection-notes-error">{errors.notes?.message}</FieldError>
                  </Field>
                </FieldGroup>
              </TabsContent>
              <TabsContent value="terminal" keepMounted className="connection-editor-panel">
                <TerminalOptions
                  value={config.terminal}
                  disabled={busy}
                  onChange={(v) => form.setValue('terminal', v, { shouldDirty: true })}
                />
              </TabsContent>
              <TabsContent value="proxy" keepMounted className="connection-editor-panel">
                <ProxyOptions
                  value={config.proxyId}
                  disabled={busy}
                  credentialBlocked={credentialBlocked}
                  onChange={(v) => form.setValue('proxyId', v, { shouldDirty: true })}
                  onChildClose={(close) => {
                    childClose.current = close;
                  }}
                />
              </TabsContent>
              <TabsContent value="tunnels" keepMounted className="connection-editor-panel">
                <TunnelOptions
                  value={config.tunnels}
                  disabled={busy}
                  connectionId={base?.id ?? null}
                  onChange={(v) => form.setValue('tunnels', v, { shouldDirty: true })}
                  onChildClose={(close) => {
                    childClose.current = close;
                  }}
                />
              </TabsContent>
            </div>
          </Tabs>
          <DialogFooter>
            <div className="connection-editor-status">
              <OperationStatus
                dismissible
                owner={owner}
                messageId={passedTestId ?? undefined}
                message={
                  error
                    ? t(error === 'draft_lost' ? 'ws_draft_lost' : errorKey(error))
                    : passedTestId
                      ? t('ws_test_passed')
                      : null
                }
                details={conflictDetails}
              />
            </div>
            <Button
              type="button"
              variant="outline"
              disabled={busy || credentialBlocked || !!unknown || !state.ready}
              onClick={() => void form.handleSubmit(test, showInvalidFields)()}
            >
              <OperationButtonIcon owner={owner} action="ws_test">
                <FlaskConicalIcon />
              </OperationButtonIcon>
              {t('ws_test')}
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={busy || !!unknown || !state.ready}
              onClick={() => void form.handleSubmit((v) => save(v, true), showInvalidFields)()}
            >
              <OperationButtonIcon owner={owner} action="ws_save_connect">
                <PlayIcon />
              </OperationButtonIcon>
              {t('ws_save_connect')}
            </Button>
            <Button type="submit" disabled={busy || !!unknown || !state.ready}>
              <OperationButtonIcon owner={owner} action="ws_save">
                <SaveIcon />
              </OperationButtonIcon>
              {t('ws_save')}
            </Button>
          </DialogFooter>
        </DialogForm>
      </DialogContent>
    </FormDialog>
  );
}

export { useConnectionEditor } from './model/use-connection-editor';

export { useConnectionSchema } from './model/use-connection-schema';
