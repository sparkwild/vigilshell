import { Progress as ProgressPrimitive } from '@base-ui/react/progress';
import { Meter as MeterPrimitive } from '@base-ui/react/meter';
import { cn } from 'cn';

const track = 'w-full overflow-hidden rounded-full bg-muted';

export function Progress({
  className,
  appearance = 'default',
  children,
  ...props
}: ProgressPrimitive.Root.Props & { appearance?: 'default' | 'transfer' }) {
  return (
    <ProgressPrimitive.Root
      data-slot="progress"
      data-appearance={appearance}
      className={cn('w-full', appearance === 'transfer' && 'relative', className)}
      {...props}
    >
      <ProgressPrimitive.Track
        className={cn(
          appearance === 'transfer'
            ? 'h-6 w-full overflow-hidden rounded-md bg-muted'
            : cn(track, 'h-1'),
        )}
      >
        <ProgressPrimitive.Indicator
          className={cn(
            'h-full',
            appearance === 'transfer'
              ? 'bg-status-success/20 data-indeterminate:w-0'
              : 'bg-status-success data-indeterminate:w-full data-indeterminate:animate-pulse motion-reduce:animate-none',
          )}
        />
      </ProgressPrimitive.Track>
      {appearance === 'transfer' ? (
        <div
          data-slot="progress-content"
          className="absolute inset-0 flex min-w-0 items-center gap-1.5 px-1.5 text-xs text-foreground tabular-nums"
        >
          {children}
        </div>
      ) : (
        children
      )}
    </ProgressPrimitive.Root>
  );
}

export function Meter({
  className,
  appearance = 'default',
  value,
  min = 0,
  max = 100,
  ...props
}: MeterPrimitive.Root.Props & { appearance?: 'default' | 'resource' }) {
  const percent = ((value - min) / (max - min)) * 100;
  const level =
    appearance === 'resource' && max > min && Number.isFinite(percent)
      ? percent >= 90
        ? 'high'
        : percent >= 70
          ? 'elevated'
          : 'normal'
      : undefined;
  return (
    <MeterPrimitive.Root
      data-slot="meter"
      data-appearance={appearance}
      data-level={level}
      value={value}
      min={min}
      max={max}
      className={cn('w-full', className)}
      {...props}
    >
      <MeterPrimitive.Track className={cn(track, 'h-1.5')}>
        <MeterPrimitive.Indicator
          data-slot="meter-indicator"
          className={cn(
            'h-full',
            level === 'high'
              ? 'bg-destructive'
              : level === 'elevated'
                ? 'bg-status-warning'
                : level === 'normal'
                  ? 'bg-status-success-foreground'
                  : 'bg-primary',
          )}
        />
      </MeterPrimitive.Track>
    </MeterPrimitive.Root>
  );
}
