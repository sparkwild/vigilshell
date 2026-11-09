import { expect, it, vi } from 'vitest';
import { readPages } from './read-pages';
import { WorkspaceFailure } from '../workspace-failure';

it('restarts a conflicted multi-page read without publishing mixed versions', async () => {
  const read = vi
    .fn()
    .mockResolvedValueOnce({ items: ['stale'], version: 1, next: 50 })
    .mockRejectedValueOnce(new WorkspaceFailure('conflict'))
    .mockResolvedValueOnce({ items: Array.from({ length: 50 }, (_, i) => i), version: 2, next: 50 })
    .mockResolvedValueOnce({
      items: Array.from({ length: 13 }, (_, i) => i + 50),
      version: 2,
      next: null,
    });
  expect(await readPages(read)).toEqual(Array.from({ length: 63 }, (_, i) => i));
  expect(read.mock.calls).toEqual([
    [0, null],
    [50, 1],
    [0, null],
    [50, 2],
  ]);
});

it('bounds conflict retries and rejects an offset that cannot advance', async () => {
  const conflict = vi.fn().mockRejectedValue(new WorkspaceFailure('conflict'));
  await expect(readPages(conflict)).rejects.toMatchObject({ code: 'conflict' });
  expect(conflict).toHaveBeenCalledTimes(3);
  await expect(readPages(async () => ({ items: [], version: 1, next: 0 }))).rejects.toMatchObject({
    code: 'protocol_mismatch',
  });
});
