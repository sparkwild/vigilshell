import { useId, useState } from 'react';
import { PlusIcon, Trash2Icon } from 'lucide-react';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Input } from '../../components/ui/input';
import { PolicyInput } from './policy/PolicyInput';
import { PolicyField } from './policy/PolicyField';
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldLabel,
  FieldGroup,
  FieldLegend,
  FieldSet,
} from '../../components/ui/field';

export function validTarget(value: string, write = false) {
  return (
    value.startsWith('/') &&
    ![...value].some((c) => c.charCodeAt(0) < 32) &&
    !value.split('/').includes('..') &&
    !value.split('/').includes('.') &&
    !value.includes('//') &&
    (value === '/' || !value.endsWith('/')) &&
    (!write || value !== '/')
  );
}
export function TargetList({
  label,
  values,
  change,
  busy,
  write = false,
  input,
  choose,
  description,
}: {
  label: string;
  values: string[];
  change(values: string[]): void;
  busy: boolean;
  write?: boolean;
  input?: { value: string; change(value: string): void };
  choose?: () => Promise<string | null>;
  description: string;
}) {
  const { t } = useDesktop();
  const id = useId(),
    [localValue, setLocalValue] = useState('');
  const value = input?.value ?? localValue;
  const setValue = input?.change ?? setLocalValue;
  const limit = 64;
  const addValid = validTarget(value, write) && !values.includes(value) && values.length < limit;
  return (
    <PolicyField appearance="settings">
      <FieldSet className="w-full" aria-describedby={`${id}-hint`}>
        <FieldLegend className="w-full">{label}</FieldLegend>
        <FieldDescription id={`${id}-hint`}>{description}</FieldDescription>
        <FieldGroup className="gap-3">
          {choose ? (
            <Button
              variant="outline"
              className="self-start"
              disabled={busy || values.length >= limit}
              onClick={() =>
                void choose().then((path) => {
                  if (!path) return;
                  if (validTarget(path, write) && !values.includes(path)) change([...values, path]);
                  else setValue(path);
                })
              }
            >
              {t('policy_choose_directory')}
            </Button>
          ) : null}
          <FieldGroup className="gap-3">
            {values.length ? (
              values.map((target, index) => {
                const invalid = !validTarget(target, write) || values.indexOf(target) !== index;
                const itemId = `${id}-${index}`;
                return (
                  <Field
                    key={itemId}
                    orientation="responsive"
                    data-invalid={invalid}
                    data-disabled={busy}
                  >
                    <FieldContent>
                      <FieldLabel className="sr-only" htmlFor={itemId}>
                        {label} {index + 1}
                      </FieldLabel>
                      <PolicyInput
                        id={itemId}
                        value={target}
                        disabled={busy}
                        aria-invalid={invalid}
                        aria-describedby={invalid ? `${itemId}-error` : undefined}
                        onChange={(e) =>
                          change(values.map((v, i) => (i === index ? e.target.value : v)))
                        }
                      />
                      {invalid ? (
                        <FieldError id={`${itemId}-error`}>{t('agent_invalid_target')}</FieldError>
                      ) : null}
                    </FieldContent>
                    <Button
                      variant="destructive"
                      size="icon-sm"
                      disabled={busy}
                      aria-label={t('agent_remove_target', { target })}
                      onClick={() => change(values.filter((_, i) => i !== index))}
                    >
                      <Trash2Icon />
                    </Button>
                  </Field>
                );
              })
            ) : (
              <Field>
                <FieldDescription>{t('agent_no_scope')}</FieldDescription>
              </Field>
            )}
          </FieldGroup>
          <Field>
            <FieldLabel className="sr-only" htmlFor={`${id}-new`}>
              {t('agent_add_target')}
            </FieldLabel>
            <div className="flex items-center gap-2">
              <Input
                id={`${id}-new`}
                value={value}
                placeholder="/Users/me/transfers"
                disabled={busy || values.length >= limit}
                onChange={(e) => setValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.nativeEvent.isComposing && addValid) {
                    e.preventDefault();
                    change([...values, value]);
                    setValue('');
                  }
                }}
              />
              <Button
                variant="outline"
                disabled={busy || !addValid}
                onClick={() => {
                  change([...values, value]);
                  setValue('');
                }}
              >
                <PlusIcon data-icon="inline-start" />
                {t('agent_add_target')}
              </Button>
            </div>
          </Field>
        </FieldGroup>
      </FieldSet>
    </PolicyField>
  );
}
