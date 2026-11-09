import { useConfirmation } from '../../../shared/ConfirmationProvider';

import { DefaultValueHint } from '../DefaultValueHint';

import { useEffect, useState } from 'react';

import type { TunnelRule } from '@contracts';

import { useDesktop } from '../../../app/DesktopProvider';

import { Button } from '../../../components/ui/button';

import { Input } from '../../../components/ui/input';

import { Field, FieldLabel, FieldGroup, FieldError } from '../../../components/ui/field';

import {
  FormDialog,
  DialogForm,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../../../components/ui/dialog';

import { tunnelRuleSchema } from '../model/form-validation';

import { OptionSelect } from '../components/OptionSelect';

export function TunnelEditor({
  initial,
  onClose,
  onSaved,
  onChildClose,
}: {
  initial: TunnelRule;
  onClose(): void;
  onSaved(rule: TunnelRule): void;
  onChildClose(close: (() => Promise<void>) | null): void;
}) {
  const { t } = useDesktop(),
    confirm = useConfirmation(),
    [rule, setRule] = useState(() => ({
      ...initial,
      bindHost: initial.bindHost === '127.0.0.1' ? '' : initial.bindHost,
      targetHost: initial.targetHost === '127.0.0.1' ? '' : initial.targetHost,
    })),
    [invalid, setInvalid] = useState(false);
  const validation = invalid ? tunnelRuleSchema.safeParse(resolveTunnelDefaults(rule)) : null;
  const invalidFields = new Set(
    validation && !validation.success
      ? validation.error.issues.map((issue) => String(issue.path[0]))
      : [],
  );
  async function close() {
    if (
      JSON.stringify(resolveTunnelDefaults(rule)) !==
        JSON.stringify(resolveTunnelDefaults(initial)) &&
      !(await confirm({
        tone: 'warning',
        title: t('ws_discard_title'),
        description: t('ws_discard_tunnel_body'),
        confirmLabel: t('ws_confirm'),
        cancelLabel: t('ws_cancel'),
      }))
    )
      return;
    onClose();
  }
  useEffect(() => {
    onChildClose(close);
    return () => onChildClose(null);
  });
  const fields: [
    keyof Pick<TunnelRule, 'name' | 'bindHost' | 'listenPort' | 'targetHost' | 'targetPort'>,
    string,
    boolean,
  ][] = [
    ['name', t('ws_option_name'), false],
    ['bindHost', t('ws_bind_address'), false],
    ['listenPort', t('ws_listen_port'), true],
    ...(rule.kind === 'socks5'
      ? []
      : ([
          ['targetHost', t('ws_target_host'), false],
          ['targetPort', t('ws_target_port'), true],
        ] as ['targetHost' | 'targetPort', string, boolean][])),
  ];
  return (
    <FormDialog
      open
      onOpenChange={(open) => {
        if (!open) void close();
      }}
    >
      <DialogContent size="form" closeLabel={t('ws_close')}>
        <DialogHeader>
          <DialogTitle>{t(initial.name ? 'ws_edit_tunnel' : 'ws_new_tunnel')}</DialogTitle>
          <DialogDescription>{t('ws_tunnel_bind_hint')}</DialogDescription>
        </DialogHeader>
        <DialogForm
          onSubmit={(e) => {
            e.preventDefault();
            e.stopPropagation();
            const result = tunnelRuleSchema.safeParse(resolveTunnelDefaults(rule));
            if (!result.success) {
              setInvalid(true);
              document
                .getElementById('tunnel-' + String(result.error.issues[0]?.path[0] ?? 'name'))
                ?.focus();
              return;
            }
            onSaved(result.data);
          }}
        >
          <FieldGroup density="tight">
            <OptionSelect
              id="tunnel-kind"
              label={t('ws_type')}
              value={rule.kind}
              options={[
                { value: 'local', label: t('ws_tunnel_local') },
                { value: 'remote', label: t('ws_tunnel_remote') },
                { value: 'socks5', label: t('ws_tunnel_dynamic') },
              ]}
              onChange={(kind) =>
                setRule({
                  ...rule,
                  kind: kind as TunnelRule['kind'],
                  targetHost: kind === 'socks5' ? null : (rule.targetHost ?? ''),
                  targetPort: kind === 'socks5' ? null : (rule.targetPort ?? 0),
                })
              }
            />
            {fields.map(([key, label, port]) => (
              <Field key={key} data-invalid={invalidFields.has(key)}>
                <div className="flex items-center gap-1">
                  <FieldLabel htmlFor={'tunnel-' + key}>{label}</FieldLabel>
                  {key === 'bindHost' || key === 'targetHost' ? (
                    <DefaultValueHint label={label}>
                      {t(key === 'bindHost' ? 'ws_default_bind_hint' : 'ws_default_target_hint')}
                    </DefaultValueHint>
                  ) : null}
                </div>
                <Input
                  id={'tunnel-' + key}
                  aria-invalid={invalidFields.has(key)}
                  aria-describedby={invalidFields.has(key) ? `tunnel-${key}-error` : undefined}
                  placeholder={
                    {
                      name: t('ws_hint_tunnel_name'),
                      bindHost: '127.0.0.1',
                      listenPort: rule.kind === 'socks5' ? '1080' : '8080',
                      targetHost: '127.0.0.1',
                      targetPort: '80',
                    }[key]
                  }
                  type={port ? 'number' : 'text'}
                  min={port ? 1 : undefined}
                  max={port ? 65535 : undefined}
                  value={rule[key] || ''}
                  onChange={(e) =>
                    setRule({
                      ...rule,
                      [key]: port ? Number(e.target.value) : e.target.value,
                    })
                  }
                />
                <FieldError id={`tunnel-${key}-error`}>
                  {invalidFields.has(key) ? t('ui_field_invalid') : undefined}
                </FieldError>
              </Field>
            ))}
          </FieldGroup>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => void close()}>
              {t('ws_cancel')}
            </Button>
            <Button type="submit">{t('ws_save')}</Button>
          </DialogFooter>
        </DialogForm>
      </DialogContent>
    </FormDialog>
  );
}

function resolveTunnelDefaults(rule: TunnelRule): TunnelRule {
  return {
    ...rule,
    bindHost: rule.bindHost.trim() || '127.0.0.1',
    targetHost: rule.kind === 'socks5' ? null : rule.targetHost?.trim() || '127.0.0.1',
  };
}
