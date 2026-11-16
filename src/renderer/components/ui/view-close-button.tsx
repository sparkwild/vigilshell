import type { ComponentProps } from 'react';
import { XIcon } from 'lucide-react';
import { cn } from 'cn';
import { IconButton } from './icon-button';

export function ViewCloseButton({
  className,
  ...props
}: Omit<ComponentProps<typeof IconButton>, 'children' | 'size' | 'variant'>) {
  return (
    <IconButton
      {...props}
      variant="ghost"
      size="icon"
      className={cn("[&_svg:not([class*='size-'])]:size-5", className)}
    >
      <XIcon aria-hidden="true" strokeWidth={2.5} />
    </IconButton>
  );
}
