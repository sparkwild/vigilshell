import { useId, useRef, useState } from 'react';
import { errorKey } from '../connections/workspace-context';
import type { RemoteEntry } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { Button } from '../../components/ui/button';
import { Checkbox } from '../../components/ui/checkbox';
import { Input } from '../../components/ui/input';
import {
  Field,
  FieldLabel,
  FieldError,
  FieldSet,
  FieldLegend,
  FieldDescription,
} from '../../components/ui/field';
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
  SelectGroup,
} from '../../components/ui/select';
import {
  FormDialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogForm,
  DialogFooter,
} from '../../components/ui/dialog';
import {
  initialPermissionChoices,
  permissionMasks,
  permissionMode,
  parsePermissionMode,
  type PermissionChoice,
} from './file-permissions';

export function FilePermissionsDialog({
  entries,
  error,
  close,
  apply,
}: {
  entries: RemoteEntry[];
  error: string | null;
  close(): void;
  apply(value: { recursive: boolean; setMask: number; clearMask: number }): Promise<boolean>;
}) {
  const { t } = useDesktop(),
    id = useId();
  const [original] = useState(() => initialPermissionChoices(entries));
  const [choices, setChoices] = useState(original),
    [mode, setMode] = useState(permissionMode(original));
  const [explicitMode, setExplicitMode] = useState(false),
    [recursive, setRecursive] = useState(false),
    [busy, setBusy] = useState(false);
  const lock = useRef(false);
  const invalid = mode !== '' && !parsePermissionMode(mode);
  const masks = permissionMasks(choices, original, explicitMode || recursive || entries.length > 1);
  return (
    <FormDialog
      open
      onOpenChange={(open) => {
        if (!open && !lock.current) close();
      }}
    >
      <DialogContent closeLabel={t('ws_close')}>
        <DialogHeader>
          <DialogTitle>{t('files_permissions_title')}</DialogTitle>
          <DialogDescription>{t('files_permissions_help')}</DialogDescription>
        </DialogHeader>
        <DialogForm
          onSubmit={(event) => {
            event.preventDefault();
            if (invalid || lock.current || !(masks.setMask || masks.clearMask)) return;
            lock.current = true;
            setBusy(true);
            void apply({ ...masks, recursive })
              .then((saved) => {
                if (saved) close();
              })
              .finally(() => {
                lock.current = false;
                setBusy(false);
              });
          }}
        >
          <div className="grid gap-4 sm:grid-cols-3">
            {(['owner', 'group', 'other'] as const).map((group, g) => (
              <FieldSet key={group}>
                <FieldLegend>{t(`files_permissions_${group}`)}</FieldLegend>
                {(['read', 'write', 'execute'] as const).map((right, r) => {
                  const index = g * 3 + r;
                  const change = (choice: PermissionChoice) => {
                    const next = choices.map((v, i) => (i === index ? choice : v));
                    setChoices(next);
                    setMode(permissionMode(next));
                    setExplicitMode(false);
                  };
                  return (
                    <Field
                      key={right}
                      orientation={entries.length === 1 ? 'horizontal' : 'vertical'}
                    >
                      {entries.length === 1 ? (
                        <Checkbox
                          id={`${id}-${index}`}
                          checked={choices[index] === 'allow'}
                          disabled={busy}
                          onCheckedChange={(checked) => change(checked ? 'allow' : 'deny')}
                        />
                      ) : null}
                      <FieldLabel htmlFor={`${id}-${index}`}>
                        {t(`files_permissions_${right}`)}
                      </FieldLabel>
                      {entries.length > 1 ? (
                        <Select
                          value={choices[index]}
                          onValueChange={(value) => {
                            if (value) change(value);
                          }}
                          disabled={busy}
                        >
                          <SelectTrigger id={`${id}-${index}`}>
                            <SelectValue>{t(`files_permissions_${choices[index]!}`)}</SelectValue>
                          </SelectTrigger>
                          <SelectContent>
                            <SelectGroup>
                              {(['keep', 'allow', 'deny'] as const).map((choice) => (
                                <SelectItem key={choice} value={choice}>
                                  {t(`files_permissions_${choice}`)}
                                </SelectItem>
                              ))}
                            </SelectGroup>
                          </SelectContent>
                        </Select>
                      ) : null}
                    </Field>
                  );
                })}
              </FieldSet>
            ))}
          </div>
          <Field data-invalid={!!invalid}>
            <FieldLabel htmlFor={`${id}-mode`}>{t('files_permissions_mode')}</FieldLabel>
            <Input
              id={`${id}-mode`}
              value={mode}
              disabled={busy}
              inputMode="numeric"
              maxLength={3}
              placeholder={entries.length > 1 ? t('files_permissions_keep') : '000'}
              aria-invalid={!!invalid}
              onChange={(event) => {
                const value = event.target.value;
                setMode(value);
                const parsed = parsePermissionMode(value);
                if (parsed) {
                  setChoices(parsed);
                  setExplicitMode(true);
                } else if (!value) {
                  setChoices(original);
                  setExplicitMode(false);
                }
              }}
            />
            <FieldError>{invalid ? t('files_permissions_invalid') : undefined}</FieldError>
          </Field>
          {entries.some((e) => e.kind === 'directory') ? (
            <Field orientation="horizontal">
              <Checkbox
                id={`${id}-recursive`}
                checked={recursive}
                disabled={busy || entries.some((e) => e.path === '/')}
                onCheckedChange={setRecursive}
              />
              <FieldLabel htmlFor={`${id}-recursive`}>
                {t('files_permissions_recursive')}
              </FieldLabel>
            </Field>
          ) : null}
          <FieldDescription>{entries.map((e) => e.path).join('\n')}</FieldDescription>
          <FieldError>{error ? t(errorKey(error)) : undefined}</FieldError>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={busy} onClick={close}>
              {t('ws_cancel')}
            </Button>
            <Button
              type="submit"
              disabled={busy || !!invalid || !(masks.setMask || masks.clearMask)}
            >
              {t('ws_confirm')}
            </Button>
          </DialogFooter>
        </DialogForm>
      </DialogContent>
    </FormDialog>
  );
}
