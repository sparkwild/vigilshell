import { useState, type ComponentProps } from 'react';
import { ResizablePanel } from '../../components/ui/resizable';

export function FileDirectoryPanel({
  width,
  ...props
}: Omit<ComponentProps<typeof ResizablePanel>, 'defaultSize'> & {
  width: number;
}) {
  // 按本次挂载读取记忆宽度；面板尚未登记布局时不能调用 imperative resize。
  const [initialWidth] = useState(width);
  return <ResizablePanel {...props} defaultSize={`${initialWidth}px`} />;
}
