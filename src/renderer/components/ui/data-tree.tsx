import { Button as AriaButton, type ButtonProps } from 'react-aria-components';

// React Aria 的 Collection 与槽位按钮必须共享原上下文，不能替换为普通 Button。
export {
  Tree,
  TreeItem,
  TreeItemContent,
  Collection,
  Virtualizer,
  ListLayout,
  useDragAndDrop,
  DropIndicator,
} from 'react-aria-components';
export type { Key, DropTarget } from 'react-aria-components';

export function Button({
  appearance = 'default',
  ...props
}: ButtonProps & {
  appearance?: 'default' | 'sidebar-group' | 'sidebar-drag';
}) {
  return <AriaButton data-slot="data-tree-button" data-appearance={appearance} {...props} />;
}
