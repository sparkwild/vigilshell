import { createContext, useContext, type ComponentProps, type ReactNode } from 'react';
import {
  Field,
  FieldContent,
  FieldControl,
  FieldDescription,
  FieldLabel,
} from '../../../components/ui/field';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../components/ui/tooltip';

export const PolicyLockContext = createContext<string | undefined>(undefined);

export function PolicyField({
  reason,
  ...props
}: ComponentProps<typeof Field> & { reason?: string }) {
  const lockedReason = useContext(PolicyLockContext);
  const hint = lockedReason ?? reason;
  return (
    <Tooltip disabled={!hint} trackCursorAxis="both">
      <TooltipTrigger
        data-slot="field"
        render={<Field {...props} />}
        tabIndex={hint ? 0 : undefined}
      />
      {hint ? <TooltipContent>{hint}</TooltipContent> : null}
    </Tooltip>
  );
}

export function PolicyRow({
  id,
  label,
  description,
  children,
  disabled = false,
  invalid = false,
  reason,
}: {
  id: string;
  label: string;
  description?: string;
  children: ReactNode;
  disabled?: boolean;
  invalid?: boolean;
  reason?: string;
}) {
  return (
    <PolicyField
      orientation="horizontal"
      appearance="settings"
      data-disabled={disabled}
      data-invalid={invalid}
      reason={reason}
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
    </PolicyField>
  );
}
