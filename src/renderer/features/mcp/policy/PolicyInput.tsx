import { useState, type ComponentProps } from 'react';
import { Input } from '../../../components/ui/input';

/** Keep partially typed values local; only commit on blur/Enter, as settings inputs do. */
export function PolicyInput({ value, onChange, ...props }: ComponentProps<typeof Input>) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <Input
      {...props}
      value={draft ?? value}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={(e) => {
        if (draft !== null) {
          onChange?.(e);
          setDraft(null);
        }
        props.onBlur?.(e);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          setDraft(null);
          e.preventDefault();
        }
        if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
          e.preventDefault();
          e.currentTarget.blur();
        }
        props.onKeyDown?.(e);
      }}
    />
  );
}
