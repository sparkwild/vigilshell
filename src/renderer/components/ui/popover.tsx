import { Popover as Primitive } from '@base-ui/react/popover';
import { cn } from 'cn';
export const Popover = Primitive.Root;
export const PopoverTrigger = Primitive.Trigger;
export function PopoverTitle({
  className,
  appearance = 'default',
  ...props
}: Primitive.Title.Props & { appearance?: 'default' | 'caption' }) {
  return (
    <Primitive.Title
      className={cn(
        appearance === 'caption'
          ? 'm-0 text-xs font-medium text-muted-foreground'
          : 'text-sm font-medium',
        className,
      )}
      {...props}
    />
  );
}
export function PopoverContent({
  className,
  align = 'start',
  appearance = 'default',
  portalContainer,
  positionerProps,
  ...props
}: Primitive.Popup.Props &
  Pick<Primitive.Positioner.Props, 'align'> & {
    appearance?: 'default' | 'navigation' | 'sessions';
    portalContainer?: Primitive.Portal.Props['container'];
    positionerProps?: Omit<Primitive.Positioner.Props, 'children' | 'className' | 'render'>;
  }) {
  return (
    <Primitive.Portal container={portalContainer}>
      <Primitive.Positioner
        sideOffset={8}
        align={align}
        className={appearance === 'navigation' ? undefined : 'z-50'}
        {...positionerProps}
      >
        <Primitive.Popup
          data-slot="popover-content"
          data-appearance={appearance}
          className={cn(
            appearance === 'navigation'
              ? 'outline-none'
              : 'z-50 w-72 max-w-(--available-width) max-h-(--available-height) overflow-y-auto rounded-lg border bg-popover p-3 text-sm text-popover-foreground shadow-md outline-none',
            className,
          )}
          {...props}
        />
      </Primitive.Positioner>
    </Primitive.Portal>
  );
}
