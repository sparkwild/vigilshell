import type { RemoteEntry } from '@contracts';
export const transferActive = (phase: string) =>
  ['queued', 'running', 'awaitingDecision', 'cancelling'].includes(phase);
export const validFileName = (name: string) =>
  !!name && name !== '.' && name !== '..' && !/[/\0]/.test(name);
export const parentPath = (path: string) => path.replace(/\/+$/, '').replace(/\/[^/]*$/, '') || '/';
export function fileSize(n: number | null) {
  if (n === null) return '—';
  if (n < 1024) return `${n} B`;
  for (const [unit, divisor] of [
    ['GiB', 1073741824],
    ['MiB', 1048576],
    ['KiB', 1024],
  ] as const)
    if (n >= divisor) return `${(n / divisor).toFixed(1)} ${unit}`;
  return '—';
}
export function permissions(entry: RemoteEntry) {
  if (entry.permissions === null) return '—';
  const mode = entry.permissions;
  const bits = [256, 128, 64, 32, 16, 8, 4, 2, 1];
  const chars = bits.map((b, i) => (mode & b ? 'rwx'[i % 3] : '-'));
  for (const [index, bit, yes, no] of [
    [2, 2048, 's', 'S'],
    [5, 1024, 's', 'S'],
    [8, 512, 't', 'T'],
  ] as const)
    if (mode & bit) chars[index] = chars[index] === 'x' ? yes : no;
  return (entry.kind === 'directory' ? 'd' : entry.kind === 'symlink' ? 'l' : '-') + chars.join('');
}
export function sortFiles(entries: RemoteEntry[], column: string, descending: boolean) {
  return [...entries].sort((a, b) => {
    if ((a.kind === 'directory') !== (b.kind === 'directory'))
      return a.kind === 'directory' ? -1 : 1;
    const av = column === 'size' ? a.size : column === 'modified' ? a.modified : a.name;
    const bv = column === 'size' ? b.size : column === 'modified' ? b.modified : b.name;
    if (av === null || bv === null) return av === bv ? 0 : av === null ? 1 : -1;
    const order =
      typeof av === 'number' && typeof bv === 'number'
        ? av - bv
        : String(av).localeCompare(String(bv), undefined, { numeric: true });
    return (descending ? -order : order) || a.name.localeCompare(b.name);
  });
}
