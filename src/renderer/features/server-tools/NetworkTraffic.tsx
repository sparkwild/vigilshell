import { useState } from 'react';
import { Bar, BarChart, XAxis, YAxis, ReferenceLine } from 'recharts';
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from '../../components/ui/card';
import { Skeleton } from '../../components/ui/skeleton';
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from '../../components/ui/chart';
import type { ServerSample } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import type { MonitorPoint } from './monitor-history';
import { networkBars, networkRate } from './network-chart';

export function NetworkTraffic({
  sample,
  points,
  collecting,
}: {
  sample?: ServerSample;
  points: MonitorPoint[];
  collecting: boolean;
}) {
  const { t } = useDesktop();
  const [chartWidth, setChartWidth] = useState(320);
  const { data, extent } = networkBars(points, chartWidth);
  const pending =
    collecting &&
    sample?.networkReceive == null &&
    sample?.networkSend == null &&
    points.length < 2;
  const config: ChartConfig = {
    send: { label: t('tools_send'), color: 'var(--chart-send)' },
    receive: { label: t('tools_receive'), color: 'var(--chart-receive)' },
  };
  const end = points.at(-1)?.time ?? Date.now();
  return (
    <Card size="sm" role="region" aria-label={t('tools_network')} className="gap-2 min-w-0">
      <CardHeader className="flex flex-wrap items-baseline justify-between gap-2">
        <CardTitle>{t('tools_network')}</CardTitle>
        <CardDescription className="tabular-nums">
          {pending
            ? t('tools_sampling')
            : `↓ ${networkRate(sample?.networkReceive)}  ↑ ${networkRate(sample?.networkSend)}`}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {data.length ? (
          <ChartContainer
            config={config}
            onResize={setChartWidth}
            className="h-24 w-full min-w-0"
            aria-label={t('tools_trend')}
          >
            <BarChart
              data={data}
              stackOffset="sign"
              barSize={3}
              margin={{ top: 4, right: 4, bottom: 4, left: 4 }}
              accessibilityLayer
            >
              <XAxis dataKey="time" type="number" domain={[end - 300000, end]} hide />
              <YAxis type="number" domain={[-extent, extent]} hide />
              <ReferenceLine y={0} stroke="var(--border)" />
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    labelFormatter={(_, payload) =>
                      `${new Date(Number(payload[0]?.payload.from)).toLocaleTimeString()}–${new Date(Number(payload[0]?.payload.time)).toLocaleTimeString()} · ${t('tools_network_peak')}`
                    }
                    formatter={(value, name) => (
                      <span className="tabular-nums">
                        {config[String(name)]?.label}: {networkRate(Math.abs(Number(value)))}
                      </span>
                    )}
                  />
                }
              />
              <Bar
                dataKey="send"
                stackId="network"
                fill="var(--color-send)"
                isAnimationActive={false}
              />
              <Bar
                dataKey="receive"
                stackId="network"
                fill="var(--color-receive)"
                isAnimationActive={false}
              />
            </BarChart>
          </ChartContainer>
        ) : (
          <Skeleton
            className="h-24 w-full motion-reduce:animate-none"
            style={pending ? undefined : { animation: 'none' }}
            aria-label={t('tools_trend')}
            aria-busy={pending}
          />
        )}
      </CardContent>
    </Card>
  );
}
