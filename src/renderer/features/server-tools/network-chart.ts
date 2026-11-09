import type { MonitorPoint } from './monitor-history';
export function networkBars(points: MonitorPoint[], width = 320) {
  const valid = (value: number | null) =>
    value != null && Number.isFinite(value) && value >= 0 ? value : null;
  const raw = points
    .map((point) => {
      const send = valid(point.send);
      const receive = valid(point.receive);
      return {
        time: point.time,
        send,
        receive: receive === null ? null : receive === 0 ? 0 : -receive,
      };
    })
    .filter((point) => point.send !== null || point.receive !== null);
  const end = points.at(-1)?.time ?? 0;
  const buckets = Math.max(1, Math.floor(width / 4));
  const bucketMs = 300000 / buckets;
  const grouped = new Map<number, (typeof raw)[number] & { from: number }>();
  for (const point of raw) {
    const bucket = Math.floor((point.time - (end - 300000)) / bucketMs);
    const old = grouped.get(bucket);
    grouped.set(
      bucket,
      old
        ? {
            time: point.time,
            from: old.from,
            send:
              old.send === null
                ? point.send
                : point.send === null
                  ? old.send
                  : Math.max(old.send, point.send),
            receive:
              old.receive === null
                ? point.receive
                : point.receive === null
                  ? old.receive
                  : Math.min(old.receive, point.receive),
          }
        : { ...point, from: point.time },
    );
  }
  const data = [...grouped.values()];
  const extent = Math.max(
    1,
    ...data.flatMap((point) => [point.send ?? 0, Math.abs(point.receive ?? 0)]),
  );
  return { data, extent };
}
export const networkRate = (value: number | null | undefined) =>
  value == null
    ? '—'
    : value >= 1048576
      ? `${(value / 1048576).toFixed(1)} MiB/s`
      : value >= 1024
        ? `${(value / 1024).toFixed(1)} KiB/s`
        : `${Math.round(value)} B/s`;
