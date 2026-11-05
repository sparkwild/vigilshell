import { expect, it } from 'vitest';
import type { RemoteEntry } from '@contracts';
import { fileSize, permissions, sortFiles } from './file-format';
const entry = (name: string, patch: Partial<RemoteEntry> = {}): RemoteEntry => ({
  name,
  path: `/${name}`,
  kind: 'file',
  size: 18,
  modified: null,
  permissions: 0o644,
  uid: 0,
  gid: 0,
  ...patch,
});
it('preserves tiny and zero sizes and special permission bits', () => {
  expect(fileSize(18)).toBe('18 B');
  expect(fileSize(0)).toBe('0 B');
  expect(fileSize(null)).toBe('—');
  expect(permissions(entry('a', { permissions: 0o4755 }))).toBe('-rwsr-xr-x');
  expect(permissions(entry('a', { kind: 'directory', permissions: 0o1700 }))).toBe('drwx-----T');
});
it('keeps folders first and null sizes last when descending', () => {
  const values = [
    entry('unknown', { size: null }),
    entry('small'),
    entry('dir', { kind: 'directory' }),
    entry('large', { size: 1000 }),
  ];
  expect(sortFiles(values, 'size', true).map((e) => e.name)).toEqual([
    'dir',
    'large',
    'small',
    'unknown',
  ]);
});
