import { expect, it } from 'vitest';
import { networkBars, networkRate } from './network-chart';
import type { MonitorPoint } from './monitor-history';
const point = (time: number, send: number | null, receive: number | null): MonitorPoint => ({
  time,
  send,
  receive,
  cpu: null,
  memory: null,
});
it('mirrors only the display values and uses the same scale for both directions', () => {
  const original = [
    point(1000, 100, 400),
    point(2000, 0, 0),
    point(3000, null, null),
    point(4000, null, 20),
  ];
  const { data, extent } = networkBars(original, 1600);
  expect(data.map(({ time, send, receive }) => ({ time, send, receive }))).toEqual([
    { time: 1000, send: 100, receive: -400 },
    { time: 2000, send: 0, receive: 0 },
    { time: 4000, send: null, receive: -20 },
  ]);
  expect(extent).toBe(400);
  expect(original[0]?.receive).toBe(400);
  expect(networkRate(Math.abs(data[0]!.receive!))).toBe('400 B/s');
});
it('keeps missing and invalid readings distinct from zero and bounds an empty scale', () => {
  expect(networkBars([point(1, NaN, -20)])).toEqual({ data: [], extent: 1 });
  expect(networkBars([point(1, 0, 0)]).data).toHaveLength(1);
  expect(networkRate(null)).toBe('—');
  expect(networkRate(0)).toBe('0 B/s');
});

it('bounds dense real-time bars by panel width and preserves peaks and source history', () => {
  const source = Array.from({ length: 301 }, (_, i) => point(i * 1000, i === 25 ? 9999 : 1, 10));
  const narrow = networkBars(source, 240);
  const wide = networkBars(source, 800);
  expect(narrow.data.length).toBeLessThanOrEqual(61);
  expect(wide.data.length).toBeGreaterThan(narrow.data.length);
  expect(narrow.extent).toBe(9999);
  expect(source).toHaveLength(301);
});
