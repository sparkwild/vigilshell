import type { ProtectedPath } from '@contracts';
import { validTarget } from '../TargetList';

/** Directory input may end in one slash; stored policy paths remain canonical. */
export function normalizeProtectedPath(path: ProtectedPath): ProtectedPath {
  return path.kind === 'directory' &&
    path.path.length > 1 &&
    path.path.endsWith('/') &&
    !path.path.endsWith('//')
    ? { ...path, path: path.path.slice(0, -1) }
    : path;
}

export function validProtectedPath(path: ProtectedPath) {
  return validTarget(path.path) && (path.kind !== 'file' || path.path !== '/');
}
