import { NetworkTraffic } from './NetworkTraffic';
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from '../../components/ui/card';
import { Skeleton } from '../../components/ui/skeleton';
import { Meter } from '../../components/ui/progress';
import { Bar, BarChart, XAxis, YAxis, CartesianGrid } from 'recharts';
import { useState } from 'react';
import { metricBars } from './monitor-history';
import {
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from '../../components/ui/chart';
import { useDesktop } from '../../app/DesktopProvider';
import type { ServerSample } from '@contracts';
import type { MonitorPoint } from './monitor-history';
const percent = (value: number | null | undefined) =>
  value == null ? '—' : `${value.toFixed(1)}%`;
const bytes = (value: number) =>
  value >= 1073741824
    ? `${(value / 1073741824).toFixed(1)} GiB`
    : `${(value / 1048576).toFixed(1)} MiB`;
export function MonitorTrends({
  sample,
  points,
  collecting = false,
}: {
  sample?: ServerSample;
  points: MonitorPoint[];
  collecting?: boolean;
}) {
  const { t, snapshot } = useDesktop();
  const display = snapshot.preferences?.monitorDisplay ?? 'bar';
  const [chartWidth, setChartWidth] = useState(320);
  const memory =
    sample?.memoryTotal && sample.memoryAvailable != null
      ? Math.max(0, sample.memoryTotal - sample.memoryAvailable)
      : null;
  const configs: {
    title: string;
    text: string;
    series: ChartConfig;
    value: number | null | undefined;
  }[] = [
    {
      title: 'CPU',
      value: sample?.cpuPercent,
      text: percent(sample?.cpuPercent),
      series: { cpu: { label: 'CPU', color: 'var(--primary)' } },
    },
    {
      title: t('tools_memory'),
      value: memory != null && sample?.memoryTotal ? (100 * memory) / sample.memoryTotal : null,
      text:
        memory != null && sample?.memoryTotal
          ? `${bytes(memory)} / ${bytes(sample.memoryTotal)} · ${percent((100 * memory) / sample.memoryTotal)}`
          : '—',
      series: { memory: { label: t('tools_memory'), color: 'var(--primary)' } },
    },
  ];
  const end = points.at(-1)?.time ?? Date.now();
  return (
    <>
      {configs.map((config) => {
        const keys = Object.keys(config.series) as (keyof Omit<MonitorPoint, 'time'>)[];
        const data = metricBars(points, keys[0] as 'cpu' | 'memory', chartWidth);
        const hasHistory = data.length > 0;
        const missing = config.value == null;
        const pending = collecting && missing && points.length < 2;
        const placeholder = (
          <Skeleton
            className={
              display === 'chart'
                ? 'h-24 w-full motion-reduce:animate-none'
                : 'h-1.5 w-full motion-reduce:animate-none'
            }
            style={pending ? undefined : { animation: 'none' }}
            aria-label={display === 'chart' ? t('tools_trend') : config.title}
            aria-busy={pending}
          />
        );
        return (
          <Card
            key={config.title}
            size="sm"
            role="region"
            aria-label={config.title}
            className="gap-2 min-w-0"
          >
            <CardHeader className="flex flex-wrap items-baseline justify-between gap-2">
              <CardTitle>{config.title}</CardTitle>
              <CardDescription className="tabular-nums">
                {pending ? t('tools_sampling') : config.text}
              </CardDescription>
            </CardHeader>
            <CardContent>
              {display === 'chart' ? (
                hasHistory ? (
                  <ChartContainer
                    config={config.series}
                    onResize={setChartWidth}
                    className="h-24 w-full min-w-0"
                    aria-label={t('tools_trend')}
                  >
                    <BarChart
                      data={data}
                      barSize={3}
                      margin={{ top: 4, right: 4, bottom: 0, left: 0 }}
                      accessibilityLayer
                    >
                      <CartesianGrid vertical={false} />
                      <XAxis dataKey="time" type="number" domain={[end - 300000, end]} hide />
                      <YAxis domain={[0, 100]} hide />
                      <ChartTooltip
                        content={
                          <ChartTooltipContent
                            labelFormatter={(_, payload) =>
                              `${new Date(Number(payload[0]?.payload.from)).toLocaleTimeString()}–${new Date(Number(payload[0]?.payload.time)).toLocaleTimeString()} · ${t('tools_network_peak')}`
                            }
                            formatter={(value, name) => (
                              <span className="tabular-nums">
                                {config.series[name as keyof typeof config.series]?.label}:{' '}
                                {percent(Number(value))}
                              </span>
                            )}
                          />
                        }
                      />
                      {Object.keys(config.series).map((key) => (
                        <Bar
                          key={key}
                          dataKey="value"
                          name={key}
                          fill={`var(--color-${key})`}
                          isAnimationActive={false}
                        />
                      ))}
                    </BarChart>
                  </ChartContainer>
                ) : (
                  placeholder
                )
              ) : config.value != null ? (
                <Meter
                  appearance="resource"
                  aria-label={config.title}
                  value={Math.max(0, Math.min(100, config.value))}
                  min={0}
                  max={100}
                />
              ) : (
                placeholder
              )}
            </CardContent>
          </Card>
        );
      })}
      <NetworkTraffic sample={sample} points={points} collecting={collecting} />
    </>
  );
}
