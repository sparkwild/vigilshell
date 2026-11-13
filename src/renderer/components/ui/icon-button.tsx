import type { ComponentProps, ReactNode } from 'react';
import { XIcon } from 'lucide-react';
import { Button } from './button';
import { Tooltip, TooltipTrigger, TooltipContent } from './tooltip';

type IconButtonProps = ComponentProps<typeof Button> & {
  label: string;
  tooltip?: ReactNode;
  side?: ComponentProps<typeof TooltipContent>['side'];
};

export function IconButton({
  label,
  tooltip = label,
  side,
  disabled,
  children,
  ...props
}: IconButtonProps) {
  const button = (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      disabled={disabled}
      {...props}
    >
      {children}
    </Button>
  );
  return (
    <Tooltip>
      {disabled ? (
        <TooltipTrigger
          key="disabled"
          render={
            <span className="inline-flex" tabIndex={0} aria-label={label} aria-disabled="true" />
          }
        >
          {button}
        </TooltipTrigger>
      ) : (
        <TooltipTrigger key="enabled" render={button} />
      )}
      <TooltipContent side={side}>{tooltip}</TooltipContent>
    </Tooltip>
  );
}

export function CloseButton({
  size = 'icon-sm',
  ...props
}: Omit<IconButtonProps, 'children' | 'variant' | 'size'> & {
  size?: 'icon-xs' | 'icon-sm' | 'icon';
}) {
  return (
    <IconButton {...props} variant="ghost" size={size}>
      <XIcon aria-hidden="true" />
    </IconButton>
  );
}
