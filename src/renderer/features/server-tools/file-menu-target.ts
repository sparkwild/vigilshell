import type { RemoteEntry } from '@contracts';
import { parentPath } from './file-format';

export interface FileMenuTarget {
  source: 'tree' | 'list' | 'blank';
  directory: string;
  entries: RemoteEntry[];
  runtimeId: string | null;
  groupId: string;
}

export function fileMenuTarget(
  source: FileMenuTarget['source'],
  directory: string,
  entry: RemoteEntry | null,
  selected: RemoteEntry[],
  runtimeId: string | null,
  groupId: string,
): FileMenuTarget {
  return {
    source,
    directory: source === 'tree' ? (entry?.path ?? '/') : directory,
    entries:
      source === 'blank' || !entry
        ? []
        : source === 'list' && selected.some((e) => e.path === entry.path)
          ? [...selected]
          : [entry],
    runtimeId,
    groupId,
  };
}

export function renamedPath(entry: RemoteEntry, name: string) {
  return `${parentPath(entry.path).replace(/\/$/, '')}/${name}`;
}
