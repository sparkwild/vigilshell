import type { ComponentProps } from 'react';
import { Button } from '../components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip';

export function ActionButton({
  disabled,
  disabledReason,
  children,
  ...props
}: ComponentProps<typeof Button> & { disabledReason?: string }) {
  return (
    <Tooltip disabled={!disabled || !disabledReason}>
      <TooltipTrigger
        render={<Button {...props} disabled={disabled} focusableWhenDisabled={!!disabledReason} />}
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{disabledReason}</TooltipContent>
    </Tooltip>
  );
}
