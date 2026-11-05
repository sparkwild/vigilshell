import { expect, it } from 'vitest';
import type { GroupLayout } from '@contracts';
import { layoutAfterDrop } from './group-layout';
const layout: GroupLayout = {
  revision: 7,
  groupIds: ['a', 'b'],
  buckets: [
    { groupId: 'a', connectionIds: ['hidden', 'one', 'hidden-after'] },
    { groupId: 'b', connectionIds: ['two'] },
    { groupId: null, connectionIds: ['loose'] },
  ],
};
it('moves relative to visible anchors without losing hidden members', () => {
  const moved = layoutAfterDrop(layout, 'c:one', {
    type: 'item',
    key: 'c:two',
    dropPosition: 'after',
  })!;
  expect(moved.buckets[0]!.connectionIds).toEqual(['hidden', 'hidden-after']);
  expect(moved.buckets[1]!.connectionIds).toEqual(['two', 'one']);
  expect(layout.buckets[0]!.connectionIds).toEqual(['hidden', 'one', 'hidden-after']);
});
it('supports head, end, ungrouped and group reordering while rejecting illegal destinations', () => {
  expect(
    layoutAfterDrop(layout, 'c:one', {
      type: 'item',
      key: 'f:b',
      dropPosition: 'on',
    })!.buckets[1]!.connectionIds,
  ).toEqual(['one', 'two']);
  expect(
    layoutAfterDrop(layout, 'c:one', {
      type: 'item',
      key: 'end:ungrouped',
      dropPosition: 'on',
    })!.buckets[2]!.connectionIds,
  ).toEqual(['loose', 'one']);
  expect(
    layoutAfterDrop(layout, 'f:a', {
      type: 'item',
      key: 'u:root',
      dropPosition: 'before',
    })!.groupIds,
  ).toEqual(['b', 'a']);
  expect(
    layoutAfterDrop(layout, 'f:a', {
      type: 'item',
      key: 'c:two',
      dropPosition: 'on',
    }),
  ).toBeNull();
  expect(
    layoutAfterDrop(layout, 'c:one', {
      type: 'item',
      key: 'c:one',
      dropPosition: 'before',
    }),
  ).toBeNull();
});
