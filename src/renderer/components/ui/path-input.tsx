import { Button } from './button';
import { Input } from './input';
import { IconButton } from './icon-button';

type PathInputProps = {
  path: string;
  value: string;
  editing: boolean;
  disabled: boolean;
  label: string;
  editLabel: string;
  onValueChange(value: string): void;
  onEditingChange(editing: boolean): void;
  onNavigate(path: string): void;
};

export function PathInput({
  path,
  value,
  editing,
  disabled,
  label,
  editLabel,
  onValueChange,
  onEditingChange,
  onNavigate,
}: PathInputProps) {
  const cancel = () => {
    onValueChange(path);
    onEditingChange(false);
  };
  const begin = () => {
    if (!disabled) {
      onValueChange(path);
      onEditingChange(true);
    }
  };
  return (
    <form
      data-slot="path-input"
      data-disabled={disabled || undefined}
      onSubmit={(event) => {
        event.preventDefault();
        if (!disabled) onNavigate(value);
      }}
    >
      {editing ? (
        <Input
          variant="path"
          autoFocus
          aria-label={label}
          value={value}
          disabled={disabled}
          onChange={(event) => onValueChange(event.target.value)}
          onBlur={cancel}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && (event.nativeEvent.isComposing || event.keyCode === 229))
              event.preventDefault();
            if (event.key === 'Escape' && !event.nativeEvent.isComposing) {
              cancel();
              event.stopPropagation();
            }
          }}
        />
      ) : (
        <div data-slot="path-breadcrumb" onDoubleClick={begin}>
          <Button
            variant="breadcrumb"
            size="breadcrumb"
            className="border-0 px-0"
            disabled={disabled}
            onClick={() => onNavigate('/')}
          >
            /
          </Button>
          {path
            .split('/')
            .filter(Boolean)
            .map((name, index, parts) => (
              <Button
                key={index}
                variant="breadcrumb"
                size="breadcrumb"
                className="border-0 px-0"
                disabled={disabled}
                onClick={() => onNavigate('/' + parts.slice(0, index + 1).join('/'))}
              >
                {name}
                {index < parts.length - 1 ? '/' : ''}
              </Button>
            ))}
          <span data-slot="path-edit">
            <IconButton
              label={editLabel}
              variant="hint"
              size="icon-xs"
              disabled={disabled}
              onClick={begin}
            >
              …
            </IconButton>
          </span>
        </div>
      )}
    </form>
  );
}
