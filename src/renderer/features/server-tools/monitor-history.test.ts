import { expect, it } from 'vitest';
import type { ServerSample } from '@contracts';
import { appendMonitorPoint, metricBars } from './monitor-history';
const sample = (collectedAt: number, cpuPercent: number | null = 10): ServerSample => ({
  collectedAt,
  cpuPercent,
  memoryTotal: 100,
  memoryAvailable: 40,
  networkReceive: 10,
  networkSend: 20,
  hostname: null,
  distribution: null,
  architecture: null,
  kernel: null,
  uptime: null,
  disks: [],
  error: null,
  phase: 'ready',
});
it('deduplicates snapshots, bounds history and leaves a gap after a sampling restart', () => {
  const first = appendMonitorPoint([], sample(1000));
  expect(appendMonitorPoint(first, sample(1000))).toBe(first);
  expect(appendMonitorPoint(first, { ...sample(2000), error: 'timeout' })).toBe(first);
  const resumed = appendMonitorPoint(first, sample(6000, null));
  expect(resumed[1]).toEqual({ time: 5999, cpu: null, memory: null, receive: null, send: null });
  expect(resumed[2]?.memory).toBe(60);
  expect(appendMonitorPoint(resumed, sample(400000))).toHaveLength(1);
});

it('retains five minutes at one-second cadence while expiring older points', () => {
  let history: ReturnType<typeof appendMonitorPoint> = [];
  for (let i = 1; i <= 400; i++) history = appendMonitorPoint(history, sample(i * 1000));
  expect(history).toHaveLength(301);
  expect(history[0]?.time).toBe(100000);
});

it('renders positive CPU/memory columns without filling missing samples and keeps peaks', () => {
  const history = [
    { time: 1000, cpu: 10, memory: null, receive: null, send: null },
    { time: 2000, cpu: 90, memory: 20, receive: null, send: null },
    { time: 15000, cpu: null, memory: null, receive: null, send: null },
  ];
  expect(metricBars(history, 'cpu', 100)).toEqual([{ from: 1000, time: 2000, value: 90 }]);
  expect(metricBars(history, 'memory', 100)).toEqual([{ from: 2000, time: 2000, value: 20 }]);
  expect(history[0]?.cpu).toBe(10);
});
