'use client';

import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip';
import { cn } from 'cn';
import { createContext, useContext, useId, useLayoutEffect, useRef, useState } from 'react';
const TooltipContext = createContext<{
  open: boolean;
  id: string;
  defaultId: string;
  setId(id: string): void;
} | null>(null);

function TooltipProvider({ delay = 0, ...props }: TooltipPrimitive.Provider.Props) {
  return <TooltipPrimitive.Provider data-slot="tooltip-provider" delay={delay} {...props} />;
}

function Tooltip({
  open: controlledOpen,
  defaultOpen = false,
  onOpenChange,
  children,
  ...props
}: TooltipPrimitive.Root.Props) {
  const defaultId = useId();
  const [id, setId] = useState(defaultId);
  const [uncontrolledOpen, setOpen] = useState(defaultOpen);
  const open = controlledOpen ?? uncontrolledOpen;
  return (
    <TooltipContext value={{ open, id, defaultId, setId }}>
      <TooltipPrimitive.Root
        data-slot="tooltip"
        {...props}
        open={open}
        onOpenChange={(next, details) => {
          onOpenChange?.(next, details);
          if (!details.isCanceled) setOpen(next);
        }}
      >
        {children}
      </TooltipPrimitive.Root>
    </TooltipContext>
  );
}

function TooltipTrigger({
  'aria-describedby': describedBy,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Trigger>) {
  const context = useContext(TooltipContext);
  const description =
    [
      ...new Set(
        [...(describedBy?.split(/\s+/) ?? []), context?.open ? context.id : undefined].filter(
          Boolean,
        ),
      ),
    ].join(' ') || undefined;
  return (
    <TooltipPrimitive.Trigger
      data-slot="tooltip-trigger"
      aria-describedby={description}
      {...props}
    />
  );
}

function TooltipContent({
  className,
  side = 'top',
  sideOffset = 4,
  align = 'center',
  alignOffset = 0,
  children,
  id: explicitId,
  ...props
}: TooltipPrimitive.Popup.Props &
  Pick<TooltipPrimitive.Positioner.Props, 'align' | 'alignOffset' | 'side' | 'sideOffset'>) {
  const context = useContext(TooltipContext);
  const id = explicitId ?? context?.defaultId;
  const setId = context?.setId;
  useLayoutEffect(() => {
    if (id) setId?.(id);
  }, [id, setId]);
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Positioner
        align={align}
        alignOffset={alignOffset}
        side={side}
        sideOffset={sideOffset}
        className="isolate z-50"
      >
        <TooltipPrimitive.Popup
          id={id}
          role="tooltip"
          data-slot="tooltip-content"
          className={cn(
            'z-50 inline-flex w-fit max-w-xs origin-(--transform-origin) items-center gap-1.5 rounded-md bg-foreground px-3 py-1.5 text-xs text-background has-data-[slot=kbd]:pr-1.5 data-[side=bottom]:slide-in-from-top-2 data-[side=inline-end]:slide-in-from-left-2 data-[side=inline-start]:slide-in-from-right-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 **:data-[slot=kbd]:relative **:data-[slot=kbd]:isolate **:data-[slot=kbd]:z-50 **:data-[slot=kbd]:rounded-sm data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:zoom-in-95 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95',
            className,
          )}
          {...props}
        >
          {children}
          <TooltipPrimitive.Arrow className="z-50 size-2.5 translate-y-[calc(-50%-2px)] rotate-45 rounded-[2px] bg-foreground fill-foreground data-[side=bottom]:top-1 data-[side=inline-end]:top-1/2! data-[side=inline-end]:-left-1 data-[side=inline-end]:-translate-y-1/2 data-[side=inline-start]:top-1/2! data-[side=inline-start]:-right-1 data-[side=inline-start]:-translate-y-1/2 data-[side=left]:top-1/2! data-[side=left]:-right-1 data-[side=left]:-translate-y-1/2 data-[side=right]:top-1/2! data-[side=right]:-left-1 data-[side=right]:-translate-y-1/2 data-[side=top]:-bottom-2.5" />
        </TooltipPrimitive.Popup>
      </TooltipPrimitive.Positioner>
    </TooltipPrimitive.Portal>
  );
}

/** 仅在文字被截断时显示骨架提示，避免浏览器原生 title 产生另一套浮层。 */
function TruncatedText({
  children,
  className,
  variant = 'default',
}: {
  children: string;
  className?: string;
  variant?: 'default' | 'navigation' | 'secondary';
}) {
  const element = useRef<HTMLSpanElement>(null);
  const [truncated, setTruncated] = useState(false);
  useLayoutEffect(() => {
    const span = element.current;
    if (!span) return;
    let disposed = false;
    const measure = () => {
      if (!disposed) setTruncated(span.scrollWidth > span.clientWidth);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(span);
    document.fonts?.ready.then(measure);
    document.fonts?.addEventListener('loadingdone', measure);
    return () => {
      disposed = true;
      observer.disconnect();
      document.fonts?.removeEventListener('loadingdone', measure);
    };
  }, [children, className, variant]);
  return (
    <Tooltip disabled={!truncated}>
      <TooltipTrigger
        ref={(node: HTMLElement | null) => {
          element.current = node;
        }}
        render={
          <span
            className={cn(
              variant === 'navigation' && 'text-[length:var(--font-size-tree)]',
              variant === 'secondary' && 'text-xs text-muted-foreground',
              className,
            )}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>{children}</TooltipContent>
    </Tooltip>
  );
}

export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider, TruncatedText };
