import * as React from 'react';
import { Input as InputPrimitive } from '@base-ui/react/input';
import { cn } from 'cn';

function Input({
  className,
  type,
  variant = 'default',
  ...props
}: React.ComponentProps<'input'> & {
  variant?: 'default' | 'inline' | 'sidebar' | 'filter' | 'path';
}) {
  return (
    <InputPrimitive
      type={type}
      data-slot="input"
      data-variant={variant}
      className={cn(
        'h-8 w-full min-w-0 rounded-lg border px-2.5 py-1 text-base transition-colors outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 md:text-sm dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40',
        variant === 'sidebar'
          ? 'h-7 border-sidebar-border bg-transparent text-xs text-sidebar-foreground hover:bg-sidebar-hover focus-visible:border-sidebar-ring focus-visible:ring-1 focus-visible:ring-sidebar-ring focus-visible:ring-inset disabled:bg-sidebar-hover md:text-xs'
          : 'border-input bg-transparent focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:bg-input/50 dark:bg-input/30 dark:disabled:bg-input/80',
        variant === 'inline' && 'h-6 rounded-sm px-1 py-0 text-xs md:text-xs',
        variant === 'filter' && 'h-6 text-xs md:text-xs',
        variant === 'path' &&
          'h-[26px] rounded-none border-0 bg-transparent px-1.5 text-xs shadow-none outline-none focus-visible:ring-0 md:text-xs dark:bg-transparent',
        className,
      )}
      {...props}
    />
  );
}

export { Input };
