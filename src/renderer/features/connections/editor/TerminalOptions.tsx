import type { TerminalSettings } from '@contracts';

import { useDesktop } from '../../../app/DesktopProvider';

import { FieldDescription, FieldGroup } from '../../../components/ui/field';

import { OptionSelect } from '../components/OptionSelect';

export function TerminalOptions({
  value,
  onChange,
  disabled,
}: {
  value: TerminalSettings;
  onChange(value: TerminalSettings): void;
  disabled: boolean;
}) {
  const { t } = useDesktop();
  return (
    <FieldGroup density="tight">
      <OptionSelect
        id="connection-encoding"
        label={t('ws_encoding')}
        value={value.encoding}
        disabled={disabled}
        options={['utf-8', 'gb18030', 'gbk', 'big5'].map((value) => ({
          value,
          label: value.toUpperCase(),
        }))}
        onChange={(encoding) =>
          onChange({
            ...value,
            encoding: encoding as TerminalSettings['encoding'],
          })
        }
      />
      <OptionSelect
        id="connection-backspace"
        label="Backspace"
        value={value.backspace}
        disabled={disabled}
        options={[
          { value: 'del', label: 'DEL (0x7F)' },
          { value: 'bs', label: 'Ctrl-H (0x08)' },
        ]}
        onChange={(backspace) =>
          onChange({
            ...value,
            backspace: backspace as TerminalSettings['backspace'],
          })
        }
      />
      <OptionSelect
        id="connection-delete-key"
        label="Delete"
        value={value.delete}
        disabled={disabled}
        options={[
          { value: 'vt220', label: 'VT220 (ESC[3~)' },
          { value: 'del', label: 'DEL (0x7F)' },
        ]}
        onChange={(key) => onChange({ ...value, delete: key as TerminalSettings['delete'] })}
      />
      <FieldDescription>{t('ws_reconnect_settings_hint')}</FieldDescription>
    </FieldGroup>
  );
}
