import { expect, it } from 'vitest';
import type { RemoteEntry } from '@contracts';
import { fileMenuTarget, renamedPath } from './file-menu-target';
const a = { name: 'a', path: '/srv/a', kind: 'file' } as RemoteEntry;
const b = { name: 'b', path: '/srv/b', kind: 'file' } as RemoteEntry;
it('retains a selected row set and replaces it when a different row is targeted', () => {
  expect(fileMenuTarget('list', '/srv', a, [a, b], 'runtime', 'group').entries).toEqual([a, b]);
  expect(fileMenuTarget('list', '/srv', a, [b], 'runtime', 'group').entries).toEqual([a]);
});
it('blank targets discard previous selection and preserve the actual destination directory', () => {
  expect(fileMenuTarget('blank', '/srv', null, [a, b], 'runtime', 'group')).toEqual({
    source: 'blank',
    directory: '/srv',
    entries: [],
    runtimeId: 'runtime',
    groupId: 'group',
  });
});
it('tree mutations use the targeted directory rather than the open listing', () => {
  const entry: RemoteEntry = { ...a, kind: 'directory' };
  expect(fileMenuTarget('tree', '/different', entry, [b], 'runtime', 'group').directory).toBe(
    '/srv/a',
  );
  expect(renamedPath(entry, 'new')).toBe('/srv/new');
  expect(fileMenuTarget('tree', '/different', null, [], 'runtime', 'group').directory).toBe('/');
});
