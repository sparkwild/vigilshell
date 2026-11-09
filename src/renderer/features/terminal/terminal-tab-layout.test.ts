import { expect, it } from 'vitest';
import { terminalTabLayout } from './terminal-tab-layout';
const items = ['a', 'b', 'c'].map((id) => ({ id, width: 160 }));
it('uses natural widths and original order when all tabs fit', () => {
  const result = terminalTabLayout(600, items, 'c');
  expect([...result.widths]).toEqual([
    ['a', 160],
    ['b', 160],
    ['c', 160],
  ]);
  expect(result.hiddenCount).toBe(0);
});
it('uses all available space without reserving an unnecessary menu slot', () => {
  const result = terminalTabLayout(324, items.slice(0, 2), 'a');
  expect([...result.widths.values()]).toEqual([160, 160]);
  expect(result.hiddenCount).toBe(0);
  const narrow = terminalTabLayout(323.9, items.slice(0, 2), 'b');
  expect([...narrow.widths]).toEqual([['b', 160]]);
  expect(narrow.hiddenCount).toBe(1);
});
it('collects whole labels instead of squeezing four readable names into ellipses', () => {
  const four = [...items, { id: 'd', width: 160 }];
  const result = terminalTabLayout(520, four, 'd');
  expect([...result.widths]).toEqual([
    ['a', 160],
    ['d', 160],
  ]);
  expect(result.hiddenCount).toBe(2);
  expect(terminalTabLayout(652, four, 'd').hiddenCount).toBe(0);
});
it('prioritizes the active session and retains preceding sessions at natural width', () => {
  const result = terminalTabLayout(400, items, 'c');
  expect([...result.widths]).toEqual([
    ['a', 160],
    ['c', 160],
  ]);
  expect(result.hiddenCount).toBe(1);
});
it('does not inflate short labels', () => {
  const result = terminalTabLayout(
    218,
    items.map((item) => ({ ...item, width: 70 })),
    'b',
  );
  expect([...result.widths.values()]).toEqual([70, 70, 70]);
  expect(result.hiddenCount).toBe(0);
});
it('caps long labels and adapts the active tab to a narrow viewport', () => {
  expect([...terminalTabLayout(800, [{ id: 'a', width: 900 }], 'a').widths.values()]).toEqual([
    220,
  ]);
  expect([...terminalTabLayout(140, items, 'c').widths]).toEqual([['c', 80]]);
});
it('handles an empty collection and a missing active session', () => {
  expect(terminalTabLayout(200, [], 'a').hiddenCount).toBe(0);
  expect([...terminalTabLayout(200, items, 'unknown').widths.keys()]).toEqual(['a']);
});
it('keeps the total within the available width across sizes and active positions', () => {
  const many = Array.from({ length: 36 }, (_, i) => ({
    id: String(i),
    width: 65 + i * 7.7,
  }));
  for (let width = 75; width < 1500; width += 13.3)
    for (const active of ['0', '17', '35']) {
      const result = terminalTabLayout(width, many, active);
      expect(result.widths.has(active)).toBe(true);
      expect(
        [...result.widths.values()].reduce((sum, value) => sum + value, 0) +
          (result.widths.size - 1) * 4 +
          (result.hiddenCount > 0 ? 60 : 0),
      ).toBeLessThanOrEqual(width);
      expect(result.hiddenCount).toBe(36 - result.widths.size);
    }
});
