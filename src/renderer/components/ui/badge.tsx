import { mergeProps } from '@base-ui/react/merge-props';
import { useRender } from '@base-ui/react/use-render';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from 'cn';

const successBadgeClassName =
  'bg-status-success/10 text-status-success-foreground border-status-success/20';

const badgeVariants = cva(
  'group/badge inline-flex h-5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-sm border border-transparent px-2 py-0.5 text-xs font-medium whitespace-nowrap transition-all focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&>svg]:pointer-events-none [&>svg]:size-3!',
  {
    variants: {
      variant: {
        success: successBadgeClassName,
        warning: 'bg-status-warning/10 text-status-warning border-status-warning/20',
        connected: 'bg-status-success/10 text-status-connected border-status-success/20',
        'environment-development':
          'bg-environment-development/10 text-environment-development border-environment-development/20',
        'environment-test':
          'bg-environment-test/10 text-environment-test border-environment-test/20',
        'environment-staging':
          'bg-environment-staging/10 text-environment-staging border-environment-staging/20',
        'environment-production':
          'bg-environment-production/10 text-environment-production border-environment-production/20',
        default: 'bg-primary text-primary-foreground [a]:hover:bg-primary/80',
        secondary: 'bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80',
        sidebar: 'bg-sidebar-hover text-muted-foreground',
        update: successBadgeClassName,
        destructive:
          'bg-destructive/10 text-destructive focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:focus-visible:ring-destructive/40 [a]:hover:bg-destructive/20',
        outline: 'border-border text-foreground [a]:hover:bg-muted [a]:hover:text-muted-foreground',
        ghost: 'hover:bg-muted hover:text-muted-foreground dark:hover:bg-muted/50',
        link: 'text-primary underline-offset-4 hover:underline',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
);

function Badge({
  className,
  variant = 'default',
  compact = false,
  render,
  ...props
}: useRender.ComponentProps<'span'> & VariantProps<typeof badgeVariants> & { compact?: boolean }) {
  return useRender({
    defaultTagName: 'span',
    props: mergeProps<'span'>(
      {
        className: cn(badgeVariants({ variant }), compact && 'max-w-16 shrink-2', className),
      },
      props,
    ),
    render,
    state: {
      slot: 'badge',
      variant,
    },
  });
}

export { Badge, badgeVariants };
