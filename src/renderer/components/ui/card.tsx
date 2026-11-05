import * as React from 'react';
import { cn } from 'cn';

function Card({
  className,
  size = 'default',
  appearance = 'default',
  interactive = false,
  ...props
}: React.ComponentProps<'div'> & {
  interactive?: boolean;
  size?: 'default' | 'sm';
  appearance?: 'default' | 'settings';
}) {
  return (
    <div
      data-slot="card"
      data-appearance={appearance}
      data-size={size}
      data-interactive={interactive || undefined}
      className={cn(
        'group/card flex flex-col gap-(--card-spacing) overflow-hidden rounded-xl bg-card py-(--card-spacing) text-sm text-card-foreground ring-1 ring-foreground/10 [--card-spacing:--spacing(4)] has-data-[slot=card-footer]:pb-0 has-[>img:first-child]:pt-0 data-[size=sm]:[--card-spacing:--spacing(3)] data-[size=sm]:has-data-[slot=card-footer]:pb-0 *:[img:first-child]:rounded-t-xl *:[img:last-child]:rounded-b-xl',
        interactive &&
          'transition-all hover:bg-muted has-focus-visible:ring-2 has-focus-visible:ring-ring/50 motion-reduce:transition-none',
        className,
      )}
      {...props}
    />
  );
}

function CardHeader({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="card-header"
      className={cn(
        'group/card-header @container/card-header grid auto-rows-min items-start gap-1 rounded-t-xl px-(--card-spacing) has-data-[slot=card-action]:grid-cols-[1fr_auto] has-data-[slot=card-description]:grid-rows-[auto_auto] [.border-b]:pb-(--card-spacing)',
        className,
      )}
      {...props}
    />
  );
}

function CardTitle({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="card-title"
      className={cn(
        'text-base leading-snug font-medium group-data-[size=sm]/card:text-sm',
        className,
      )}
      {...props}
    />
  );
}

function CardDescription({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="card-description"
      className={cn('text-sm text-muted-foreground', className)}
      {...props}
    />
  );
}

function CardAction({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="card-action"
      className={cn('col-start-2 row-span-2 row-start-1 self-start justify-self-end', className)}
      {...props}
    />
  );
}

function CardContent({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div data-slot="card-content" className={cn('px-(--card-spacing)', className)} {...props} />
  );
}

function CardFooter({
  className,
  appearance = 'default',
  ...props
}: React.ComponentProps<'div'> & { appearance?: 'default' | 'plain' }) {
  return (
    <div
      data-slot="card-footer"
      data-appearance={appearance}
      className={cn(
        'flex items-center rounded-b-xl p-(--card-spacing)',
        appearance === 'default' && 'border-t bg-muted/50',
        appearance === 'plain' && 'pt-0',
        className,
      )}
      {...props}
    />
  );
}

export { Card, CardHeader, CardFooter, CardTitle, CardAction, CardDescription, CardContent };
