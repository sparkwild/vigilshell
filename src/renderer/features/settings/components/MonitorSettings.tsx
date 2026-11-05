import { useRef, useState } from 'react';
import type { Preferences, PreferencesPatch } from '@contracts';
import { useDesktop } from '../../../app/DesktopProvider';
import { monitorPresets } from '../../../../shared/preferences';
import { Row } from './SettingsControls';
import { Tabs, TabsList, TabsTrigger, TabsIndicator } from '../../../components/ui/tabs';
import { Slider } from '../../../components/ui/slider';

export function MonitorSettings({
  preferences: p,
  busy,
  save,
}: {
  preferences: Preferences;
  busy: boolean;
  save(patch: Omit<PreferencesPatch, 'downloadDirectory'>): Promise<boolean>;
}) {
  const { t } = useDesktop();
  const savedIndex = Math.max(
    0,
    monitorPresets.findIndex((preset) => preset.sample === p.monitorSampleSeconds),
  );
  const [preview, setPreview] = useState<number | null>(null);
  const pending = useRef(false);
  const index = preview ?? savedIndex;
  async function commit(value: number | readonly number[]) {
    const selected = Array.isArray(value) ? value[0] : (value as number);
    const preset = monitorPresets[selected];
    if (!preset || busy || pending.current) return;
    pending.current = true;
    try {
      await save({ monitorSampleSeconds: preset.sample, monitorFullSeconds: preset.full });
    } finally {
      pending.current = false;
      setPreview(null);
    }
  }
  return (
    <>
      <Row
        id="monitor-display"
        label={t('settings_monitor_display')}
        description={t('settings_monitor_display_description')}
      >
        <Tabs
          value={p.monitorDisplay}
          onValueChange={(value) => {
            if (value === 'bar' || value === 'chart') void save({ monitorDisplay: value });
          }}
        >
          <TabsList variant="segmented" shape="rounded" aria-labelledby="monitor-display-label">
            <TabsTrigger value="bar" disabled={busy}>
              {t('settings_monitor_bar')}
            </TabsTrigger>
            <TabsTrigger value="chart" disabled={busy}>
              {t('settings_monitor_chart')}
            </TabsTrigger>
            <TabsIndicator />
          </TabsList>
        </Tabs>
      </Row>
      <Row
        id="monitor-speed"
        label={t('settings_monitor_speed')}
        description={t(monitorPresets[index]!.description)}
      >
        <div className="flex w-60 max-w-full flex-col gap-3 py-1">
          <Slider
            value={[index]}
            min={0}
            max={2}
            step={1}
            thumbAlignment="center"
            marks={[0, 1, 2]}
            disabled={busy}
            thumbProps={{
              'aria-labelledby': 'monitor-speed-label',
              'aria-describedby': 'monitor-speed-description',
              getAriaValueText: (_, value) => t(monitorPresets[value]!.label),
            }}
            onValueChange={(value) =>
              setPreview(Array.isArray(value) ? value[0]! : (value as number))
            }
            onValueCommitted={(value) => void commit(value)}
          />
          <div className="grid grid-cols-3 text-xs text-muted-foreground" aria-hidden="true">
            {monitorPresets.map((preset) => (
              <span key={preset.value} className="text-center first:text-left last:text-right">
                {t(preset.label)}
              </span>
            ))}
          </div>
        </div>
      </Row>
    </>
  );
}
