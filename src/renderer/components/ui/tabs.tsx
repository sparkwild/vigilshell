import { createContext, useContext } from 'react';
import { Tabs as TabsPrimitive } from '@base-ui/react/tabs';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from 'cn';
import { AdaptiveLabelsContext, AdaptiveTabButton, AdaptiveTabsList } from './adaptive-tabs';

function Tabs({
  className,
  orientation = 'horizontal',
  appearance = 'default',
  ...props
}: TabsPrimitive.Root.Props & { appearance?: 'default' | 'workspace' }) {
  return (
    <TabsPrimitive.Root
      data-slot="tabs"
      data-appearance={appearance}
      data-orientation={orientation}
      className={cn('group/tabs flex gap-2 data-horizontal:flex-col', className)}
      {...props}
    />
  );
}

const tabsListVariants = cva(
  'group/tabs-list inline-flex w-fit items-center justify-center rounded-lg p-[3px] text-muted-foreground group-data-horizontal/tabs:h-8 group-data-vertical/tabs:h-fit group-data-vertical/tabs:flex-col data-[variant=line]:rounded-none',
  {
    variants: {
      variant: {
        default: 'bg-muted',
        editor: '',
        line: 'gap-1 bg-transparent',
        session: 'gap-1 rounded-none bg-transparent p-0',
        segmented:
          'relative isolate min-w-0 rounded-full bg-sidebar-hover p-0.5 group-data-horizontal/tabs:h-7',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  },
);

const TabsAppearance = createContext<'default' | 'line' | 'segmented' | 'session' | 'editor'>(
  'default',
);

function TabsList({
  className,
  variant = 'default',
  adaptiveLabels = false,
  shape = 'pill',
  ...props
}: TabsPrimitive.List.Props &
  VariantProps<typeof tabsListVariants> & {
    adaptiveLabels?: boolean;
    shape?: 'pill' | 'rounded';
  }) {
  const List = adaptiveLabels && variant === 'segmented' ? AdaptiveTabsList : TabsPrimitive.List;
  return (
    <TabsAppearance.Provider value={variant ?? 'default'}>
      <List
        data-slot="tabs-list"
        data-variant={variant}
        data-shape={shape}
        className={cn(
          tabsListVariants({ variant }),
          variant === 'segmented' && shape === 'rounded' && 'rounded-lg',
          className,
        )}
        {...props}
      />
    </TabsAppearance.Provider>
  );
}

function TabsTrigger({
  className,
  label,
  children,
  ...props
}: TabsPrimitive.Tab.Props & { label?: string }) {
  const adaptive = useContext(AdaptiveLabelsContext);
  const appearance = useContext(TabsAppearance);
  if (appearance === 'session')
    return (
      <TabsPrimitive.Tab
        data-slot="tabs-trigger"
        className={cn(
          'relative inline-flex h-full w-full min-w-0 items-center gap-1.5 rounded-md bg-transparent py-0 pr-8 pl-2 text-xs font-normal whitespace-nowrap text-muted-foreground outline-none hover:text-foreground data-active:text-foreground focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-inset disabled:pointer-events-none disabled:opacity-50 [&_svg]:shrink-0 [&_svg]:size-3.5',
          className,
        )}
        {...props}
      >
        {children}
        {label}
      </TabsPrimitive.Tab>
    );
  if (appearance === 'segmented')
    return (
      <TabsPrimitive.Tab
        data-slot="tabs-trigger"
        className={cn(
          'relative z-10 inline-flex h-6 min-w-0 shrink items-center justify-center gap-1 rounded-full bg-transparent py-0 pl-1.5 pr-2 text-xs font-medium whitespace-nowrap text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50 data-active:text-foreground [&_svg]:pointer-events-none [&_svg]:size-3 [&_svg]:shrink-0',
          'group-data-[shape=rounded]/tabs-list:rounded-md',
          className,
        )}
        {...props}
        render={
          adaptive && label !== undefined
            ? (nativeProps, state) => (
                <AdaptiveTabButton {...nativeProps} active={state.active} label={label} />
              )
            : props.render
        }
      >
        {children}
        {!adaptive && label !== undefined && <span className="truncate">{label}</span>}
      </TabsPrimitive.Tab>
    );
  return (
    <TabsPrimitive.Tab
      data-slot="tabs-trigger"
      className={cn(
        "relative inline-flex h-[calc(100%-1px)] flex-1 items-center justify-center gap-1.5 rounded-md border border-transparent px-1.5 py-0.5 text-sm font-medium whitespace-nowrap text-foreground/60 transition-all group-data-vertical/tabs:w-full group-data-vertical/tabs:justify-start hover:text-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1 focus-visible:outline-ring disabled:pointer-events-none disabled:opacity-50 has-data-[icon=inline-end]:pr-1 has-data-[icon=inline-start]:pl-1 aria-disabled:pointer-events-none aria-disabled:opacity-50 dark:text-muted-foreground dark:hover:text-foreground group-data-[variant=default]/tabs-list:data-active:shadow-sm group-data-[variant=line]/tabs-list:data-active:shadow-none [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        'group-data-[variant=line]/tabs-list:bg-transparent group-data-[variant=line]/tabs-list:data-active:bg-transparent dark:group-data-[variant=line]/tabs-list:data-active:border-transparent dark:group-data-[variant=line]/tabs-list:data-active:bg-transparent',
        'data-active:bg-background data-active:text-foreground dark:data-active:border-input dark:data-active:bg-input/30 dark:data-active:text-foreground',
        'after:absolute after:bg-foreground after:opacity-0 after:transition-opacity group-data-horizontal/tabs:after:inset-x-0 group-data-horizontal/tabs:after:bottom-[-5px] group-data-horizontal/tabs:after:h-0.5 group-data-vertical/tabs:after:inset-y-0 group-data-vertical/tabs:after:-right-1 group-data-vertical/tabs:after:w-0.5 group-data-[variant=line]/tabs-list:data-active:after:opacity-100',
        className,
      )}
      {...props}
    >
      {children}
      {label}
    </TabsPrimitive.Tab>
  );
}

function TabsContent({ className, ...props }: TabsPrimitive.Panel.Props) {
  return (
    <TabsPrimitive.Panel
      data-slot="tabs-content"
      className={cn('flex-1 text-sm outline-none', className)}
      {...props}
    />
  );
}

function TabsIndicator({ className, ...props }: TabsPrimitive.Indicator.Props) {
  const appearance = useContext(TabsAppearance);
  return (
    <TabsPrimitive.Indicator
      data-slot="tabs-indicator"
      className={cn(
        'pointer-events-none absolute top-0 left-0 h-(--active-tab-height) w-(--active-tab-width) [transform:translate(var(--active-tab-left),var(--active-tab-top))]',
        appearance === 'segmented'
          ? 'rounded-full group-data-[shape=rounded]/tabs-list:rounded-md bg-sidebar-selection group-has-[[aria-selected=true]:hover]/tabs-list:bg-sidebar-selection-hover transition-[transform,width,background-color] duration-200 ease-out motion-reduce:transition-none'
          : 'bg-muted transition-[transform,width,height]',
        className,
      )}
      {...props}
    />
  );
}

export { Tabs, TabsList, TabsTrigger, TabsContent, TabsIndicator, tabsListVariants };
