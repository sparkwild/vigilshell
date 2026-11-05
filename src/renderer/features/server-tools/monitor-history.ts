import type { ServerSample } from '@contracts';
export type MonitorPoint = {
  time: number;
  cpu: number | null;
  memory: number | null;
  receive: number | null;
  send: number | null;
};
export function appendMonitorPoint(history: MonitorPoint[], sample: ServerSample): MonitorPoint[] {
  if (!sample.collectedAt || sample.error || sample.phase !== 'ready') return history;
  const last = history.at(-1);
  if (last && last.time >= sample.collectedAt) return history;
  const points = history.filter((p) => p.time >= sample.collectedAt - 300_000);
  if (last && sample.cpuPercent === null)
    points.push({
      time: sample.collectedAt - 1,
      cpu: null,
      memory: null,
      receive: null,
      send: null,
    });
  points.push({
    time: sample.collectedAt,
    cpu: sample.cpuPercent,
    memory:
      sample.memoryTotal && sample.memoryAvailable !== null
        ? Math.max(0, Math.min(100, 100 * (1 - sample.memoryAvailable / sample.memoryTotal)))
        : null,
    receive: sample.networkReceive,
    send: sample.networkSend,
  });
  return points.slice(-640);
}

export function metricBars(points: MonitorPoint[], metric: 'cpu' | 'memory', width: number) {
  const end = points.at(-1)?.time ?? 0;
  const bucketMs = 300000 / Math.max(1, Math.floor(width / 4));
  const buckets = new Map<number, { from: number; time: number; value: number }>();
  for (const point of points) {
    const value = point[metric];
    if (value === null || !Number.isFinite(value)) continue;
    const key = Math.floor((point.time - end + 300000) / bucketMs);
    const old = buckets.get(key);
    buckets.set(key, {
      from: old?.from ?? point.time,
      time: point.time,
      value: Math.max(old?.value ?? 0, Math.max(0, Math.min(100, value))),
    });
  }
  return [...buckets.values()];
}
