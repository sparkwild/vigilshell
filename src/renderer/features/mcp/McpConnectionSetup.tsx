import { useId, useRef, useState } from 'react';
import { CheckIcon, CopyIcon, RefreshCwIcon, ChevronDownIcon } from 'lucide-react';
import type { AgentClient, AgentDelivery, AgentUiRequest } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { Field, FieldDescription, FieldGroup, FieldLabel } from '../../components/ui/field';
import { ToggleGroup, ToggleGroupItem } from '../../components/ui/toggle-group';
import {
  FormDialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  DialogForm,
} from '../../components/ui/dialog';
import { mcpHttpConfiguration, mcpInstructions } from '../../../shared/mcp-instructions';
import { AgentLogo, clientNames } from './McpClientPicker';
import { useConfirmation } from '../../shared/ConfirmationProvider';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '../../components/ui/collapsible';
import { McpFeedback } from './McpFeedback';
import type { useMcp } from './model';

export function McpConnectionSetup({
  entryId,
  client = 'generic',
  model,
  close,
  onCreated,
}: {
  entryId: string | null;
  client?: AgentClient;
  model: ReturnType<typeof useMcp>;
  close(): void;
  onCreated(id: string): void;
}) {
  const { t, snapshot: desktop } = useDesktop();
  const snapshot = model.snapshot!;
  const confirm = useConfirmation();
  function uniqueName(base: string) {
    let name = base;
    let suffix = 2;
    while (snapshot.entries.some((entry) => entry.name === name)) name = `${base} ${suffix++}`;
    return name;
  }
  const [targetId, setTargetId] = useState(entryId);
  const current = snapshot.entries.find((entry) => entry.id === targetId);
  const selectedClient = current?.client ?? client;
  const [name, setName] = useState(() =>
    uniqueName(
      client === 'generic'
        ? t('agent_new_name', { number: snapshot.entries.length + 1 })
        : clientNames[client],
    ),
  );
  const [renameDraft, setRenameDraft] = useState<string | null>(null);
  const [resetDone, setResetDone] = useState(false);
  const [delivery, setDelivery] = useState<AgentDelivery>('prompt');
  const [copied, setCopied] = useState<AgentDelivery | null>(null);
  const [failed, setFailed] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [requestKey] = useState(() => crypto.randomUUID());
  const [attempt, setAttempt] = useState<{
    request: Extract<AgentUiRequest, { action: 'prepareAccess' }>;
    before: Set<string>;
  } | null>(null);
  const inFlight = useRef(false);
  const nameId = useId();
  const busy = model.busy || submitting;
  const unavailable = targetId !== null && !current?.enabled;
  const atLimit = !targetId && !attempt && snapshot.entries.length >= 32;
  const locked = busy || (attempt !== null && targetId === null);
  const displayedName = targetId ? (renameDraft ?? current?.name ?? name) : name;
  const nameChanged = !!current && displayedName.trim() !== current.name;
  const endpoint = snapshot.endpoint || 'http://127.0.0.1:51602/mcp';
  const content =
    delivery === 'prompt'
      ? mcpInstructions(desktop.appearance.locale, endpoint, t('agent_prompt_placeholder'))
      : mcpHttpConfiguration(endpoint, t('agent_prompt_placeholder'));

  async function rename() {
    if (
      inFlight.current ||
      busy ||
      unavailable ||
      !current ||
      selectedClient !== 'generic' ||
      !displayedName.trim() ||
      !nameChanged
    )
      return;
    inFlight.current = true;
    setSubmitting(true);
    setFailed(false);
    try {
      if (await model.issue({ action: 'renameAccess', id: current.id, name: displayedName.trim() }))
        setRenameDraft(null);
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }

  async function rotate() {
    if (inFlight.current || busy || unavailable || !current) return;
    const accepted = await confirm({
      title: t('agent_rotate_title', { name: current.name }),
      description: t('agent_rotate_description'),
      confirmLabel: t('agent_reset_access'),
      cancelLabel: t('ws_cancel'),
      tone: 'neutral',
    });
    if (!accepted || inFlight.current) return;
    inFlight.current = true;
    setSubmitting(true);
    setFailed(false);
    setResetDone(false);
    try {
      if (await model.issue({ action: 'rotateAccess', id: current.id })) {
        setCopied(null);
        setResetDone(true);
      }
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }

  async function copy() {
    if (inFlight.current || busy || unavailable || atLimit || nameChanged || !displayedName.trim())
      return;
    inFlight.current = true;
    setSubmitting(true);
    setCopied(null);
    setFailed(false);
    try {
      let request: AgentUiRequest;
      let pending = attempt;
      if (current) {
        request = {
          action: 'deliverAccess',
          id: current.id,
          client: current.client ?? 'generic',
          delivery,
        };
      } else {
        pending ??= {
          request: {
            action: 'prepareAccess',
            name: name.trim(),
            client,
            id: null,
            delivery,
            requestKey,
          },
          before: new Set(snapshot.entries.map((entry) => entry.id)),
        };
        setAttempt(pending);
        request = pending.request;
      }
      const ok = await model.issue(request, (value) => {
        if (current) return;
        const created = value.entries.find(
          (entry) =>
            !pending!.before.has(entry.id) && entry.name === pending!.request.name && entry.enabled,
        );
        if (created) {
          setTargetId(created.id);
          onCreated(created.id);
        }
      });
      if (ok) {
        setCopied(delivery);
        setResetDone(false);
      } else setFailed(true);
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }

  return (
    <FormDialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) close();
      }}
    >
      <DialogContent
        size="form"
        showCloseButton={false}
        style={{ width: 'min(576px, calc(100vw - 32px))' }}
      >
        <DialogHeader>
          <DialogTitle>
            {targetId
              ? t('agent_configure_named', { name: current?.name ?? name })
              : t('agent_add_connection')}
          </DialogTitle>
          <DialogDescription>
            {t(
              targetId
                ? 'agent_configure_hint'
                : selectedClient === 'generic'
                  ? 'agent_add_connection_hint'
                  : 'agent_local_setup_hint',
            )}
          </DialogDescription>
        </DialogHeader>
        <DialogForm
          onSubmit={(event) => {
            event.preventDefault();
            if (nameChanged) void rename();
            else void copy();
          }}
        >
          {copied ? (
            <div className="mcp-setup-result" role="status">
              <CheckIcon aria-hidden="true" />
              <p>
                {t(
                  copied === 'prompt' ? 'agent_prompt_copy_next' : 'agent_configuration_copy_next',
                )}
              </p>
              <Button type="button" variant="ghost" onClick={() => setCopied(null)}>
                {t('agent_setup_back')}
              </Button>
            </div>
          ) : (
            <FieldGroup>
              {selectedClient !== 'generic' ? (
                <div className="flex items-center gap-2">
                  <AgentLogo client={selectedClient} />
                  <span>{current?.name ?? name}</span>
                </div>
              ) : (
                <Field orientation="horizontal">
                  <FieldLabel className="shrink-0 whitespace-nowrap" htmlFor={nameId}>
                    {t('agent_connection_name')}
                  </FieldLabel>
                  <Input
                    id={nameId}
                    value={displayedName}
                    maxLength={128}
                    disabled={locked || unavailable}
                    onChange={(event) => {
                      if (targetId) setRenameDraft(event.target.value);
                      else setName(event.target.value);
                    }}
                  />
                  {targetId ? (
                    <Button
                      type="button"
                      variant="outline"
                      disabled={locked || unavailable || !nameChanged || !displayedName.trim()}
                      onClick={() => void rename()}
                    >
                      {t('ws_save')}
                    </Button>
                  ) : null}
                </Field>
              )}
              <Field>
                <FieldLabel>{t('agent_setup_method')}</FieldLabel>
                <ToggleGroup
                  appearance="segmented"
                  size="sm"
                  value={[delivery]}
                  disabled={locked || unavailable}
                  aria-label={t('agent_setup_method')}
                  onValueChange={(values) => {
                    const value = values[0];
                    if (value === 'prompt' || value === 'configuration') {
                      setDelivery(value);
                      setCopied(null);
                    }
                  }}
                >
                  <ToggleGroupItem value="prompt">{t('agent_setup_assisted')}</ToggleGroupItem>
                  <ToggleGroupItem value="configuration">{t('agent_manual_title')}</ToggleGroupItem>
                </ToggleGroup>
                <FieldDescription>
                  {t(delivery === 'prompt' ? 'agent_prompt_hint' : 'agent_manual_hint')}
                </FieldDescription>
              </Field>
              <pre
                tabIndex={0}
                aria-label={t('agent_setup_content')}
                className="mcp-instruction-preview mcp-setup-preview"
              >
                {content}
              </pre>
              {!snapshot.settings.enabled ? (
                <FieldDescription>{t('agent_prompt_enable_hint')}</FieldDescription>
              ) : null}
              {current?.enabled ? (
                <Collapsible>
                  <CollapsibleTrigger
                    render={<Button type="button" variant="ghost" className="group" />}
                  >
                    {t('agent_advanced_settings')}
                    <ChevronDownIcon
                      data-icon="inline-end"
                      aria-hidden="true"
                      className="transition-transform duration-150 group-aria-expanded:rotate-180 motion-reduce:transition-none"
                    />
                  </CollapsibleTrigger>
                  <CollapsibleContent className="pt-3">
                    <FieldGroup>
                      <Field>
                        <FieldDescription>{t('agent_rotate_description')}</FieldDescription>
                        <Button
                          type="button"
                          variant="outline"
                          disabled={busy || unavailable}
                          onClick={() => void rotate()}
                        >
                          <RefreshCwIcon data-icon="inline-start" />
                          {t('agent_reset_access')}
                        </Button>
                      </Field>
                    </FieldGroup>
                  </CollapsibleContent>
                </Collapsible>
              ) : null}
              {resetDone ? (
                <FieldDescription role="status">{t('agent_rotate_done')}</FieldDescription>
              ) : null}
              {failed ? (
                <FieldDescription role="alert">
                  {t(
                    model.error === 'access_copy_failed' || targetId
                      ? 'agent_setup_copy_failed'
                      : 'agent_setup_add_failed',
                  )}
                </FieldDescription>
              ) : null}
              <McpFeedback
                code={
                  unavailable
                    ? 'access_revoked'
                    : model.error === 'access_copy_failed'
                      ? null
                      : model.error
                }
              />
              {atLimit ? <FieldDescription>{t('agent_connection_limit')}</FieldDescription> : null}
            </FieldGroup>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={close}>
              {t(copied ? 'agent_return_connections' : 'ws_close')}
            </Button>
            <Button
              type="submit"
              disabled={busy || unavailable || atLimit || nameChanged || !displayedName.trim()}
            >
              {copied ? (
                <CheckIcon data-icon="inline-start" />
              ) : (
                <CopyIcon data-icon="inline-start" />
              )}
              {t(
                copied
                  ? 'agent_copy_again'
                  : failed
                    ? 'agent_setup_retry'
                    : targetId
                      ? delivery === 'prompt'
                        ? 'agent_prompt_button'
                        : 'mcp_copy_config'
                      : 'agent_add_copy',
              )}
            </Button>
          </DialogFooter>
        </DialogForm>
      </DialogContent>
    </FormDialog>
  );
}
