import type { GroupLayout } from '@contracts';
import type { DropTarget, Key } from '../../components/ui/data-tree';
export function layoutAfterDrop(
  layout: GroupLayout,
  key: Key,
  target: DropTarget,
): GroupLayout | null {
  const source = String(key);
  const next = structuredClone(layout);
  const targetKey = target.type === 'item' ? String(target.key) : 'u:root';
  const position = target.type === 'item' ? target.dropPosition : 'on';
  if (source.startsWith('f:')) {
    if (
      !(targetKey.startsWith('f:') && position !== 'on') &&
      !(targetKey === 'u:root' && position === 'before')
    )
      return null;
    const id = source.slice(2),
      to = targetKey.slice(2);
    if (!next.groupIds.includes(id) || id === to) return null;
    next.groupIds = next.groupIds.filter((g) => g !== id);
    const index = targetKey === 'u:root' ? next.groupIds.length : next.groupIds.indexOf(to);
    if (index < 0) return null;
    next.groupIds.splice(index + Number(position === 'after'), 0, id);
  } else if (source.startsWith('c:')) {
    const id = source.slice(2);
    const from = next.buckets.find((b) => b.connectionIds.includes(id));
    if (!from || targetKey === source) return null;
    let groupId: string | null,
      anchor: string | null = null;
    let atEnd = false;
    if (targetKey.startsWith('c:') && position !== 'on') {
      anchor = targetKey.slice(2);
      const bucket = next.buckets.find((b) => b.connectionIds.includes(anchor!));
      if (!bucket) return null;
      groupId = bucket.groupId ?? null;
    } else if (
      position === 'on' &&
      (targetKey.startsWith('f:') ||
        targetKey === 'u:root' ||
        targetKey.startsWith('empty:') ||
        targetKey.startsWith('end:'))
    ) {
      groupId =
        targetKey === 'u:root' || targetKey.endsWith(':ungrouped')
          ? null
          : targetKey.slice(targetKey.indexOf(':') + 1);
      atEnd = targetKey.startsWith('end:') || target.type === 'root';
    } else return null;
    const to = next.buckets.find((b) => (b.groupId ?? null) === groupId);
    if (!to) return null;
    from.connectionIds = from.connectionIds.filter((c) => c !== id);
    const index = anchor
      ? to.connectionIds.indexOf(anchor) + Number(position === 'after')
      : atEnd
        ? to.connectionIds.length
        : 0;
    to.connectionIds.splice(index, 0, id);
  } else return null;
  return JSON.stringify(next) === JSON.stringify(layout) ? null : next;
}
