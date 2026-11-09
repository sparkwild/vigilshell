export interface AdaptiveTabSize {
  id: string;
  full: number;
  icon: number;
}

export interface AdaptiveTabsLayout {
  width: number;
  compact: boolean;
  innerWidth: number;
  gap: number;
  items: AdaptiveTabSize[];
}

// 以完整内容计算阈值，不能让已隐藏的文字反过来影响显示模式。
export function adaptiveTabsLayout(
  available: number,
  inset: number,
  gap: number,
  items: AdaptiveTabSize[],
): AdaptiveTabsLayout {
  const roundUp = (value: number) => Math.ceil(value * 64) / 64;
  const measured = items.map((item) => ({
    ...item,
    full: roundUp(item.full),
    icon: roundUp(item.icon),
  }));
  const spacing = inset + gap * Math.max(0, items.length - 1);
  const natural = roundUp(spacing + measured.reduce((total, item) => total + item.full, 0));
  const width = Math.max(0, Math.min(natural, Math.floor(available * 64) / 64));
  return {
    width,
    compact: natural > width,
    innerWidth: Math.max(0, width - spacing),
    gap,
    items: measured,
  };
}

export function adaptiveTabPresentation(
  layout: AdaptiveTabsLayout | null,
  id: string,
  active: boolean,
) {
  const item = layout?.items.find((entry) => entry.id === id);
  if (!layout || !item) return { width: undefined, mode: 'full' as const };
  if (!layout.compact) return { width: item.full, mode: 'full' as const };
  if (!active) return { width: item.icon, mode: 'icon' as const };
  const width = Math.max(
    0,
    layout.innerWidth -
      layout.items.reduce((sum, entry) => sum + (entry.id === id ? 0 : entry.icon), 0),
  );
  return {
    width,
    mode: width < item.full ? ('ellipsis' as const) : ('full' as const),
  };
}
