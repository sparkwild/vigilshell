import { useEffect, useLayoutEffect, useRef } from 'react';
import type { AdaptiveTabsLayout } from './adaptive-tabs-layout';

/** 仅平滑紧凑模式的内容重排，布局与滑块测量仍由原组件负责。 */
export function useAdaptiveTabMotion(
  active: boolean,
  layout: AdaptiveTabsLayout | null,
  width: number | undefined,
  label: string,
) {
  const icon = useRef<HTMLSpanElement>(null);
  const text = useRef<HTMLSpanElement>(null);
  const previous = useRef<{ active: boolean; x: number } | null>(null);
  const animations = useRef<Animation[]>([]);

  useLayoutEffect(() => {
    const element = icon.current;
    const glyph = element?.firstElementChild;
    if (!element || !glyph) return;
    const transform = getComputedStyle(element).transform;
    const offset = !transform || transform === 'none' ? 0 : new DOMMatrixReadOnly(transform).m41;
    const from = previous.current ? previous.current.x + offset : null;
    animations.current.forEach((animation) => animation.cancel());
    animations.current = [];
    const x = glyph.getBoundingClientRect().x;
    const changed = previous.current !== null && previous.current.active !== active;
    previous.current = { active, x };
    if (
      !changed ||
      !layout?.compact ||
      from === null ||
      matchMedia('(prefers-reduced-motion: reduce)').matches
    )
      return;
    const indicator = element
      .closest('[role="tablist"]')
      ?.querySelector('[data-slot="tabs-indicator"]');
    if (!indicator) return;
    const style = getComputedStyle(indicator);
    const duration =
      parseFloat(style.transitionDuration) * (style.transitionDuration.endsWith('ms') ? 1 : 1000);
    if (duration < 1 || style.transitionProperty === 'none') return;
    const timing = { duration, easing: style.transitionTimingFunction };
    if (Math.abs(from - x) > 0.5) {
      // 从当前可见位置继续，避免快速反向切换时先跳回上一次终点。
      animations.current.push(
        element.animate(
          [{ transform: `translateX(${from - x}px)` }, { transform: 'translateX(0)' }],
          timing,
        ),
      );
    }
    if (active && text.current) {
      animations.current.push(
        text.current.animate(
          [
            { opacity: 0, transform: `translateX(${from - x}px)` },
            { opacity: 1, transform: 'translateX(0)' },
          ],
          timing,
        ),
      );
    }
  }, [active, layout, width, label]);

  useEffect(() => {
    const media = matchMedia('(prefers-reduced-motion: reduce)');
    const cancel = () => animations.current.forEach((animation) => animation.cancel());
    const changed = () => {
      if (media.matches) cancel();
    };
    media.addEventListener('change', changed);
    return () => {
      media.removeEventListener('change', changed);
      cancel();
    };
  }, []);

  return { icon, text };
}
