import { Switch } from '../../../components/ui/switch';
import { PolicyRow as Row } from './PolicyField';

export function PolicySwitch({
  id,
  label,
  description,
  checked,
  busy,
  change,
  reason,
}: {
  id: string;
  label: string;
  description?: string;
  checked: boolean;
  busy: boolean;
  change(value: boolean): void;
  reason?: string;
}) {
  return (
    <Row id={id} label={label} description={description} disabled={busy} reason={reason}>
      <Switch
        id={id}
        aria-describedby={description ? `${id}-description` : undefined}
        disabled={busy}
        checked={checked}
        onCheckedChange={change}
      />
    </Row>
  );
}
