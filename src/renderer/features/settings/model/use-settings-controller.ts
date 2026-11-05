import { availableTerminalFonts, terminalFonts } from '../../terminal/terminal-fonts';

import { useEffect, useRef, useState } from 'react';

import { useDesktop } from '../../../app/DesktopProvider';

import { type MessageKey } from '../../../../shared/i18n/messages';

import { usePreferences } from '../use-preferences';

import { useDelayedBusy } from '../../../components/ui/use-delayed-busy';

import { ChoiceProps } from '../components/SettingsControls';

import { categories, type SettingsCategory } from '../settings-navigation';

export function useSettingsController(initialCategory: SettingsCategory = 'general') {
  const { store, snapshot, t } = useDesktop();
  const { preferences: p, disabled, save, error, setError, retry } = usePreferences();
  const [category, setCategory] = useState<(typeof categories)[number]['id']>(initialCategory);
  const [nativeBusy, setNativeBusy] = useState(false);
  const [fonts] = useState(() => availableTerminalFonts(document, true));
  const fontUnavailable = !fonts.some((font) => font.id === p.terminalFontFamily);
  const selectedFont =
    terminalFonts.find((font) => font.id === p.terminalFontFamily) ?? terminalFonts[0];
  const fontItems: ChoiceProps<(typeof terminalFonts)[number]['id']>['items'] = fonts.map(
    (font) => ({
      value: font.id,
      label: font.label,
    }),
  );
  if (fontUnavailable)
    fontItems.push({
      value: p.terminalFontFamily,
      label: t('settings_font_unavailable_option', {
        font: selectedFont.label,
      }),
      disabled: true,
    });
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus();
  }, [category]);
  const busy = disabled || nativeBusy;
  const pending = useDelayedBusy(
    snapshot.save.phase === 'saving' || snapshot.save.phase === 'confirming' || nativeBusy,
    'settings',
  );
  const title = t(categories.find((c) => c.id === category)!.label);
  const numeric = (values: readonly number[]) =>
    values.map((v) => ({ value: String(v), label: String(v) }));
  const options = <const T extends string>(values: [T, MessageKey][]) =>
    values.map(([value, label]) => ({ value, label: t(label) }));
  async function chooseDirectory() {
    setNativeBusy(true);
    setError(null);
    try {
      setError((await store.chooseDownloadDirectory()).error);
    } catch {
      setError('host_unavailable');
    } finally {
      setNativeBusy(false);
    }
  }
  const failure = snapshot.save.error || error;
  return {
    store,
    snapshot,
    t,
    p,
    disabled,
    save,
    category,
    setCategory,
    fontUnavailable,
    fontItems,
    heading,
    busy,
    pending,
    title,
    numeric,
    options,
    chooseDirectory,
    failure,
    retry,
  };
}
