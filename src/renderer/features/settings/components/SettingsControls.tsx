import { useEffect, useRef, useState, type ReactNode } from 'react';

import { useDesktop } from '../../../app/DesktopProvider';

import {
  Field,
  FieldContent,
  FieldControl,
  FieldDescription,
  FieldLabel,
  FieldError,
} from '../../../components/ui/field';

import { Input } from '../../../components/ui/input';

import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../../../components/ui/select';

export function Row({
  id,
  label,
  description,
  children,
  disabled = false,
  invalid = false,
}: {
  id: string;
  label: string;
  description?: string;
  children: ReactNode;
  disabled?: boolean;
  invalid?: boolean;
}) {
  return (
    <Field
      orientation="horizontal"
      appearance="settings"
      data-disabled={disabled}
      data-invalid={invalid}
    >
      <FieldContent>
        <FieldLabel id={`${id}-label`} htmlFor={id}>
          {label}
        </FieldLabel>
        {description ? (
          <FieldDescription id={`${id}-description`}>{description}</FieldDescription>
        ) : null}
      </FieldContent>
      <FieldControl>{children}</FieldControl>
    </Field>
  );
}

export type ChoiceProps<T extends string = string> = {
  id: string;
  value: T;
  items: { value: T; label: string; disabled?: boolean }[];
  disabled: boolean;
  onChange: (value: T) => void;
};

export function ChoiceSelect<T extends string>({
  id,
  value,
  items,
  disabled,
  onChange,
}: ChoiceProps<T>) {
  return (
    <Select
      items={items}
      value={value}
      disabled={disabled}
      onValueChange={(next) => {
        if (next !== null) onChange(next);
      }}
    >
      <SelectTrigger
        id={id}
        aria-labelledby={`${id}-label`}
        aria-describedby={`${id}-description`}
        size="sm"
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          {items.map((item) => (
            <SelectItem key={item.value} value={item.value} disabled={item.disabled}>
              {item.label}
            </SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  );
}

export function NumberPreference({
  id,
  label,
  description,
  value,
  min,
  max,
  step = 1,
  disabled,
  onCommit,
}: {
  id: string;
  label: string;
  description: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  disabled: boolean;
  onCommit: (value: number) => void;
}) {
  const { t } = useDesktop();
  const [draft, setDraft] = useState(String(value)),
    [invalid, setInvalid] = useState(false);
  const skipBlur = useRef(false);
  useEffect(() => {
    setDraft(String(value));
    setInvalid(false);
  }, [value]);
  const commit = () => {
    if (skipBlur.current) {
      skipBlur.current = false;
      return;
    }
    const n = Number(draft);
    if (
      !draft.trim() ||
      !Number.isFinite(n) ||
      n < min ||
      n > max ||
      Math.abs((n - min) / step - Math.round((n - min) / step)) > 0.00001
    ) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    onCommit(n);
  };
  return (
    <Row id={id} label={label} description={description} invalid={invalid} disabled={disabled}>
      <div className="flex min-w-0 flex-col items-end gap-1">
        <Input
          id={id}
          aria-labelledby={`${id}-label`}
          aria-describedby={[`${id}-description`, invalid ? `${id}-error` : null]
            .filter(Boolean)
            .join(' ')}
          type="number"
          className="w-24"
          min={min}
          max={max}
          step={step}
          value={draft}
          aria-invalid={invalid}
          disabled={disabled}
          onChange={(e) => {
            skipBlur.current = false;
            setDraft(e.target.value);
          }}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing || e.keyCode === 229) return;
            if (e.key === 'Enter') {
              e.preventDefault();
              commit();
              skipBlur.current = true;
            }
            if (e.key === 'Escape') {
              e.preventDefault();
              setDraft(String(value));
              setInvalid(false);
              skipBlur.current = true;
            }
          }}
        />
        <FieldError id={`${id}-error`}>
          {invalid ? t('settings_number_range', { min, max, step }) : undefined}
        </FieldError>
      </div>
    </Row>
  );
}
