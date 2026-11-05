import { useLayoutEffect, useRef, useState } from 'react';
import { terminalTabLayout, type TerminalTabSize } from './terminal-tab-layout';

export function useTerminalTabLayout(ids: string[], activeId: string, geometry: string) {
  const viewport = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [measurements, setMeasurements] = useState<{
    available: number;
    items: TerminalTabSize[];
  }>({ available: 0, items: [] });
  useLayoutEffect(() => {
    const container = viewport.current?.parentElement,
      content = list.current;
    if (!container || !content) return;
    let frame = 0,
      disposed = false;
    const number = (value: string) => parseFloat(value) || 0;
    const measure = () => {
      frame = 0;
      if (disposed || container.clientWidth <= 0) return;
      // 隐藏或省略的标签仍测量原始文字，避免显示结果反过来改变收纳阈值。
      const items = Array.from(content.querySelectorAll<HTMLElement>('[role="tab"]')).map((tab) => {
        const css = getComputedStyle(tab);
        const probe = tab.querySelector<HTMLElement>('.terminal-tab-label-measure')!;
        const icon = tab.querySelector('svg, .mcp-client-logo');
        const badge = tab.querySelector<HTMLElement>('[data-slot="badge"]');
        const chrome =
          number(css.paddingLeft) +
          number(css.paddingRight) +
          number(css.borderLeftWidth) +
          number(css.borderRightWidth);
        const iconWidth = icon ? number(getComputedStyle(icon).width) : 0;
        let badgeWidth = 0;
        if (badge) {
          const badgeCss = getComputedStyle(badge);
          const text = badge.querySelector<HTMLElement>('[data-slot="tooltip-trigger"]');
          badgeWidth = Math.min(
            64,
            (text?.scrollWidth ?? badge.scrollWidth) +
              number(badgeCss.paddingLeft) +
              number(badgeCss.paddingRight) +
              number(badgeCss.borderLeftWidth) +
              number(badgeCss.borderRightWidth),
          );
        }
        return {
          id: tab.dataset.sessionId!,
          width:
            chrome +
            iconWidth +
            probe.getBoundingClientRect().width +
            badgeWidth +
            number(css.columnGap) * ((icon ? 1 : 0) + (badge ? 1 : 0)),
        };
      });
      const next = { available: container.clientWidth, items };
      setMeasurements((previous) =>
        JSON.stringify(previous) === JSON.stringify(next) ? previous : next,
      );
    };
    const schedule = () => {
      if (!disposed && !frame) frame = requestAnimationFrame(measure);
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(container);
    content
      .querySelectorAll('.terminal-tab-label-measure, [data-slot="badge"], .mcp-client-logo')
      .forEach((node) => observer.observe(node));
    document.fonts?.ready.then(schedule);
    document.fonts?.addEventListener('loadingdone', schedule);
    measure();
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      observer.disconnect();
      document.fonts?.removeEventListener('loadingdone', schedule);
    };
  }, [geometry]);
  const natural = new Map(measurements.items.map((item) => [item.id, item.width]));
  const layout = terminalTabLayout(
    measurements.available,
    ids.map((id) => ({ id, width: natural.get(id) ?? 160 })),
    activeId,
  );
  return { viewport, list, ...layout, natural };
}
