import { errorCode } from '../workspace-context';

import { WorkspaceFailure } from '../workspace-store';

export async function readPages<T>(
  read: (
    offset: number,
    version: number | null,
  ) => Promise<{ items: T[]; version: number; next: number | null }>,
): Promise<T[]> {
  for (let attempt = 0; attempt < 3; attempt++) {
    const items: T[] = [];
    let offset = 0,
      version: number | null = null;
    try {
      for (;;) {
        const page = await read(offset, version);
        items.push(...page.items);
        if (page.next === null) return items;
        if (page.next <= offset) throw new WorkspaceFailure('protocol_mismatch');
        offset = page.next;
        version = page.version;
      }
    } catch (e) {
      if (errorCode(e) !== 'conflict' || attempt === 2) throw e;
    }
  }
  throw new WorkspaceFailure('conflict');
}
