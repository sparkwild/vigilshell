import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from '../../../components/ui/context-menu';

export function RowContextMenu({
  children,
  menu,
  finalFocus,
}: {
  children: React.ReactElement;
  menu: React.ReactNode;
  finalFocus: React.ComponentProps<typeof ContextMenuContent>['finalFocus'];
}) {
  if (!menu) return children;
  return (
    <ContextMenu>
      <ContextMenuTrigger render={children} />
      <ContextMenuContent finalFocus={finalFocus}>{menu}</ContextMenuContent>
    </ContextMenu>
  );
}
