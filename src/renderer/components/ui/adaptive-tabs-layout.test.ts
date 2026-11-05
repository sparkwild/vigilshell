import { describe, expect, it } from 'vitest';
import { adaptiveTabPresentation, adaptiveTabsLayout } from './adaptive-tabs-layout';

const items = [
  { id: 'connections', full: 104, icon: 26 },
  { id: 'groups', full: 74, icon: 26 },
];

describe('adaptive tab allocation', () => {
  it('uses natural width when both names fit', () => {
    const layout = adaptiveTabsLayout(220, 4, 0, items);
    expect(layout.width).toBe(182);
    expect(layout.compact).toBe(false);
    expect(adaptiveTabPresentation(layout, 'groups', false)).toEqual({
      width: 74,
      mode: 'full',
    });
  });
  it('keeps the outer width stable and gives remaining space to either active item', () => {
    const layout = adaptiveTabsLayout(160, 4, 0, items);
    for (const item of items) {
      expect(adaptiveTabPresentation(layout, item.id, true)).toEqual({
        width: 130,
        mode: 'full',
      });
      expect(adaptiveTabPresentation(layout, item.id, false)).toEqual({
        width: 26,
        mode: 'icon',
      });
    }
    expect(layout.width).toBe(160);
  });
  it('allows the selected name to overflow only after reserving the other icon', () => {
    const layout = adaptiveTabsLayout(160, 4, 0, [{ id: 'long', full: 470, icon: 26 }, items[1]!]);
    expect(adaptiveTabPresentation(layout, 'long', true)).toEqual({
      width: 130,
      mode: 'ellipsis',
    });
    expect(adaptiveTabPresentation(layout, 'groups', true)).toEqual({
      width: 130,
      mode: 'full',
    });
  });
  it('restores all names at the intrinsic-width boundary without mode feedback', () => {
    expect(adaptiveTabsLayout(181.5, 4, 0, items).compact).toBe(true);
    expect(adaptiveTabsLayout(182, 4, 0, items).compact).toBe(false);
  });
  it('accounts for changed font metrics, padding and gaps without language rules', () => {
    const large = items.map((item) => ({
      ...item,
      full: item.full * 1.5,
      icon: item.icon * 1.5,
    }));
    const layout = adaptiveTabsLayout(220, 8, 6, large);
    expect(layout.compact).toBe(true);
    expect(adaptiveTabPresentation(layout, 'connections', true).width).toBe(167);
  });
});
