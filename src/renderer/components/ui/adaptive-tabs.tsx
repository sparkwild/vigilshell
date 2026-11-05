import {
  createContext,
  useContext,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentPropsWithRef,
} from 'react';
import { Tabs as TabsPrimitive } from '@base-ui/react/tabs';
import { useRender } from '@base-ui/react/use-render';
import { Tooltip, TooltipContent, TooltipTrigger } from './tooltip';
import { useAdaptiveTabMotion } from './use-adaptive-tab-motion';
import {
  adaptiveTabPresentation,
  adaptiveTabsLayout,
  type AdaptiveTabsLayout,
} from './adaptive-tabs-layout';

export const AdaptiveLabelsContext = createContext(false);
const LayoutContext = createContext<AdaptiveTabsLayout | null>(null);

export function AdaptiveTabsList({ ref, children, style, ...props }: TabsPrimitive.List.Props) {
  const viewport = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [layout, setLayout] = useState<AdaptiveTabsLayout | null>(null);

  useLayoutEffect(() => {
    const container = viewport.current;
    const element = list.current;
    if (!container || !element) return;
    let disposed = false;
    const number = (value: string) => parseFloat(value) || 0;
    function measure() {
      if (disposed || !container || !element || container.clientWidth === 0) return;
      const listStyle = getComputedStyle(element);
      const inset =
        number(listStyle.paddingLeft) +
        number(listStyle.paddingRight) +
        number(listStyle.borderLeftWidth) +
        number(listStyle.borderRightWidth);
      const items = Array.from(element.querySelectorAll<HTMLElement>('[role="tab"]')).flatMap(
        (tab) => {
          const probe = tab.querySelector<HTMLElement>('[data-slot="tabs-label-measure"]');
          const icon = tab.querySelector<HTMLElement>('[data-slot="tabs-icon"]')?.firstElementChild;
          if (!probe) return [];
          const css = getComputedStyle(tab);
          const chrome =
            number(css.paddingLeft) +
            number(css.paddingRight) +
            number(css.borderLeftWidth) +
            number(css.borderRightWidth);
          const iconWidth = icon ? number(getComputedStyle(icon).width) : 0;
          const textWidth = number(getComputedStyle(probe).width);
          return [
            {
              id: tab.id,
              icon: chrome + iconWidth,
              full: chrome + iconWidth + (icon ? number(css.columnGap) : 0) + textWidth,
            },
          ];
        },
      );
      if (!items.length) return;
      const next = adaptiveTabsLayout(
        number(getComputedStyle(container).width),
        inset,
        number(listStyle.columnGap),
        items,
      );
      setLayout((previous) =>
        JSON.stringify(previous) === JSON.stringify(next) ? previous : next,
      );
    }
    const resize = new ResizeObserver(measure);
    function observe() {
      resize.disconnect();
      resize.observe(container!);
      resize.observe(element!);
      element!
        .querySelectorAll('[data-slot="tabs-label-measure"], [data-slot="tabs-icon"] > *')
        .forEach((node) => resize.observe(node));
      measure();
    }
    const mutation = new MutationObserver(observe);
    mutation.observe(element, {
      childList: true,
      characterData: true,
      subtree: true,
    });
    observe();
    document.fonts?.ready.then(measure);
    document.fonts?.addEventListener('loadingdone', measure);
    return () => {
      disposed = true;
      mutation.disconnect();
      resize.disconnect();
      document.fonts?.removeEventListener('loadingdone', measure);
    };
  }, []);

  const renderedList = useRender({
    render: <TabsPrimitive.List />,
    ref: [list, ref ?? null],
    props: {
      ...props,
      'data-adaptive-labels': '',
      'data-label-layout': layout?.compact ? 'compact' : 'full',
      style: { ...style, width: layout?.width, maxWidth: '100%' },
      children: (
        <LayoutContext.Provider value={layout}>
          <AdaptiveLabelsContext.Provider value>{children}</AdaptiveLabelsContext.Provider>
        </LayoutContext.Provider>
      ),
    },
  });
  // 将胶囊作为 Flex 子项，避免其文字基线变化撑高工具栏并推移下方内容。
  return (
    <div
      ref={viewport}
      data-slot="adaptive-tabs-viewport"
      className="flex min-w-0 flex-1 items-center"
    >
      {renderedList}
    </div>
  );
}

export function AdaptiveTabButton({
  active,
  label,
  children,
  ref,
  style,
  ...props
}: ComponentPropsWithRef<'button'> & { active: boolean; label: string }) {
  const layout = useContext(LayoutContext);
  const [open, setOpen] = useState(false);
  const tooltipId = useId();
  const presentation = adaptiveTabPresentation(layout, props.id ?? '', active);
  const needsTooltip = presentation.mode !== 'full';
  const motion = useAdaptiveTabMotion(active, layout, presentation.width, label);
  const button = useRender({
    defaultTagName: 'button',
    ref,
    props: {
      ...props,
      title: undefined,
      'aria-label': label,
      'aria-describedby':
        needsTooltip && open
          ? [props['aria-describedby'], tooltipId].filter(Boolean).join(' ')
          : props['aria-describedby'],
      'data-label-mode': presentation.mode,
      style: {
        ...style,
        width: presentation.width,
        flexShrink: layout ? 0 : undefined,
        overflow: active && layout?.compact ? 'hidden' : undefined,
      },
      children: (
        <>
          <span
            ref={motion.icon}
            data-slot="tabs-icon"
            className={
              presentation.mode === 'icon'
                ? 'pointer-events-none absolute inset-0 flex items-center justify-center'
                : 'inline-flex shrink-0'
            }
          >
            {children}
          </span>
          <span
            ref={motion.text}
            data-slot="tabs-label"
            className="min-w-0 truncate"
            hidden={presentation.mode === 'icon'}
          >
            {label}
          </span>
          <span
            aria-hidden="true"
            className="pointer-events-none invisible absolute inset-0 overflow-hidden"
          >
            <span data-slot="tabs-label-measure" className="block w-max whitespace-nowrap">
              {label}
            </span>
          </span>
        </>
      ),
    },
  });
  return (
    <Tooltip open={open} disabled={!needsTooltip} onOpenChange={setOpen}>
      <TooltipTrigger data-slot="tabs-trigger" render={button} disabled={!needsTooltip} />
      <TooltipContent id={tooltipId} role="tooltip" className="[overflow-wrap:anywhere]">
        {label}
      </TooltipContent>
    </Tooltip>
  );
}
