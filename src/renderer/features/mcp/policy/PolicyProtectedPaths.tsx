import { PlusIcon, Trash2Icon } from 'lucide-react';
import type { ProtectedPath } from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import { Button } from '../../../components/ui/button';
import { PolicyInput as Input } from './PolicyInput';
import { FieldGroup, FieldLabel, FieldError } from '../../../components/ui/field';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../../components/ui/select';
import { PolicyField as Field } from './PolicyField';
import { normalizeProtectedPath, validProtectedPath } from './protected-path';

export function PolicyProtectedPaths({
  paths,
  busy,
  change,
}: {
  paths: ProtectedPath[];
  busy: boolean;
  change(paths: ProtectedPath[]): void;
}) {
  const { t } = useDesktop();
  const update = (index: number, value: Partial<ProtectedPath>) =>
    change(paths.map((p, i) => (i === index ? normalizeProtectedPath({ ...p, ...value }) : p)));
  return (
    <FieldGroup className="min-w-0 p-4">
      {paths.map((p, index) => {
        const duplicate = paths.some((other, i) => i !== index && other.path === p.path);
        const invalid = !!p.path && (!validProtectedPath(p) || duplicate);
        const pathLabel = t(p.kind === 'file' ? 'policy_file_path' : 'policy_directory_path');
        const hintId = `protected-path-${index}-error`;
        return (
          <Field key={index} className="min-w-0" data-invalid={invalid}>
            <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-start gap-2 @md/field-group:grid-cols-[7rem_minmax(0,1fr)_auto]">
              <Select
                disabled={busy}
                value={p.kind}
                items={{ directory: t('policy_directory'), file: t('policy_file') }}
                onValueChange={(kind) => {
                  if (kind === 'directory' || kind === 'file') update(index, { kind });
                }}
              >
                <SelectTrigger className="w-full" aria-label={t('policy_path_type')}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="directory">{t('policy_directory')}</SelectItem>
                    <SelectItem value="file">{t('policy_file')}</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
              <div className="col-start-1 row-start-2 min-w-0 @md/field-group:col-start-2 @md/field-group:row-start-1">
                <FieldLabel className="sr-only" htmlFor={`protected-path-${index}`}>
                  {pathLabel}
                </FieldLabel>
                <Input
                  id={`protected-path-${index}`}
                  value={p.path}
                  placeholder={t(
                    p.kind === 'file'
                      ? 'policy_file_path_placeholder'
                      : 'policy_directory_path_placeholder',
                  )}
                  disabled={busy}
                  aria-invalid={invalid}
                  aria-describedby={invalid ? hintId : undefined}
                  onChange={(e) => update(index, { path: e.target.value })}
                />
                {invalid ? (
                  <FieldError id={hintId} className="mt-1">
                    {t(
                      duplicate
                        ? 'policy_path_duplicate'
                        : p.kind === 'file'
                          ? 'policy_file_path_invalid'
                          : 'policy_directory_path_invalid',
                    )}
                  </FieldError>
                ) : null}
              </div>
              <Button
                className="col-start-2 row-start-1 shrink-0 @md/field-group:col-start-3"
                size="icon-sm"
                variant="destructive"
                disabled={busy}
                aria-label={t('agent_remove_target', { target: p.path || pathLabel })}
                onClick={() => change(paths.filter((_, i) => i !== index))}
              >
                <Trash2Icon />
              </Button>
            </div>
          </Field>
        );
      })}
      <Field orientation="horizontal" className="flex-wrap">
        <Button
          variant="outline"
          className="max-w-full"
          disabled={busy || paths.length >= 128}
          onClick={() => change([...paths, { path: '', kind: 'directory' }])}
        >
          <PlusIcon data-icon="inline-start" />
          {t('policy_add_protected_path')}
        </Button>
      </Field>
    </FieldGroup>
  );
}
