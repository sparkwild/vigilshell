import { Button as ButtonPrimitive } from '@base-ui/react/button';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from 'cn';

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center rounded-lg border border-transparent bg-clip-padding text-sm font-medium whitespace-nowrap transition-all outline-none select-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:not-aria-disabled:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground not-aria-disabled:hover:bg-primary/80',
        warning:
          'border-status-warning/25 bg-status-warning/15 text-status-warning not-aria-disabled:hover:bg-status-warning/25 focus-visible:border-status-warning/40 focus-visible:ring-status-warning/20',
        outline:
          'border-border bg-background not-aria-disabled:hover:bg-muted not-aria-disabled:hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:border-input dark:bg-input/30 dark:not-aria-disabled:hover:bg-input/50',
        secondary:
          'bg-secondary text-secondary-foreground not-aria-disabled:hover:bg-[color-mix(in_oklch,var(--secondary),var(--foreground)_5%)] aria-expanded:bg-secondary aria-expanded:text-secondary-foreground',
        ghost:
          'not-aria-disabled:hover:bg-muted not-aria-disabled:hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:not-aria-disabled:hover:bg-muted/50',
        'ghost-contrast':
          'not-aria-disabled:hover:bg-foreground/10 not-aria-disabled:hover:text-foreground aria-expanded:bg-foreground/10 aria-expanded:text-foreground',
        sidebar:
          'text-[length:var(--font-size-body)] font-normal text-sidebar-foreground not-aria-disabled:hover:bg-sidebar-hover active:not-aria-disabled:bg-sidebar-pressed aria-expanded:bg-sidebar-hover',
        update:
          'text-update-accent not-aria-disabled:hover:bg-sidebar-hover active:not-aria-disabled:bg-sidebar-pressed',
        'sidebar-placeholder':
          'border-dashed border-sidebar-border bg-transparent font-normal text-muted-foreground not-aria-disabled:hover:bg-sidebar-hover active:not-aria-disabled:bg-sidebar-pressed',
        titlebar:
          'text-muted-foreground not-aria-disabled:hover:bg-sidebar-hover not-aria-disabled:hover:text-sidebar-foreground active:not-aria-disabled:bg-sidebar-pressed',
        destructive:
          'bg-destructive/10 text-destructive not-aria-disabled:hover:bg-destructive/20 focus-visible:border-destructive/40 focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:not-aria-disabled:hover:bg-destructive/30 dark:focus-visible:ring-destructive/40',
        link: 'text-primary underline-offset-4 not-aria-disabled:hover:underline',
        'panel-toggle':
          'rounded-none border-t border-border text-left not-aria-disabled:hover:bg-muted aria-expanded:bg-transparent aria-expanded:not-aria-disabled:hover:bg-muted data-[active]:bg-muted',
        'quiet-toggle':
          'not-aria-disabled:hover:bg-muted not-aria-disabled:hover:text-foreground aria-expanded:bg-transparent aria-expanded:not-aria-disabled:hover:bg-muted',
        hint: 'text-muted-foreground not-aria-disabled:hover:bg-muted not-aria-disabled:hover:text-foreground',
        'tab-close':
          'opacity-55 not-aria-disabled:hover:opacity-100 focus-visible:opacity-100 group-hover/session-tab:opacity-100 group-focus-within/session-tab:opacity-100 not-aria-disabled:hover:bg-muted not-aria-disabled:hover:text-foreground focus-visible:bg-muted focus-visible:text-foreground active:not-aria-disabled:bg-accent',
        breadcrumb: 'rounded-none font-normal not-aria-disabled:hover:bg-muted',
      },
      size: {
        default:
          'h-8 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2',
        xs: "h-6 gap-1 rounded-[min(var(--radius-md),10px)] px-2 text-xs in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-7 gap-1 rounded-[min(var(--radius-md),12px)] px-2.5 text-[0.8rem] in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3.5",
        lg: 'h-9 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2',
        compact: 'h-7 gap-1 px-2.5 text-xs',
        list: 'h-auto min-h-10 flex-1 min-w-0 justify-start gap-1 p-1 text-left text-xs font-normal whitespace-normal',
        strip: 'h-[26px] justify-start gap-1.5 px-2 text-xs',
        breadcrumb: 'h-[26px] shrink min-w-0 px-[3px] py-0 text-xs overflow-hidden text-ellipsis',
        'icon-hint': 'size-5 [&_svg]:size-3.5',
        'icon-tab': 'size-6 [&_svg]:size-3.5',
        'icon-close': "size-8 [&_svg:not([class*='size-'])]:size-5",
        icon: 'size-8',
        'icon-xs':
          "size-6 rounded-[min(var(--radius-md),10px)] in-data-[slot=button-group]:rounded-lg [&_svg:not([class*='size-'])]:size-3",
        'icon-sm':
          'size-7 rounded-[min(var(--radius-md),12px)] in-data-[slot=button-group]:rounded-lg',
        'icon-lg': 'size-9',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  },
);

function Button({
  className,
  variant = 'default',
  size = 'default',
  ...props
}: ButtonPrimitive.Props & VariantProps<typeof buttonVariants>) {
  return (
    <ButtonPrimitive
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  );
}

export { Button, buttonVariants };
