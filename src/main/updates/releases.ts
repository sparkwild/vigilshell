import type { AvailableRelease, UpdateSnapshot } from '@contracts';
// Release versions follow the project's stable X.Y.Z grammar.
function version(value: string): number[] | null {
  const m = /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value);
  if (!m) return null;
  const parts = [Number(m[1]), Number(m[2]), Number(m[3])];
  return parts.every(Number.isSafeInteger) ? parts : null;
}
function compare(a: number[], b: number[]) {
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i]! > b[i]! ? 1 : -1;
  }
  return 0;
}
export function selectRelease(
  data: unknown,
  current: string,
  platform: string,
  arch: string,
): UpdateSnapshot {
  if (!Array.isArray(data) || !version(current)) throw Error('invalidResponse');
  const installed = version(current)!;
  const releases: AvailableRelease[] = [];
  for (const item of data) {
    if (
      !item ||
      typeof item !== 'object' ||
      typeof item.tag_name !== 'string' ||
      typeof item.draft !== 'boolean' ||
      typeof item.prerelease !== 'boolean' ||
      !Array.isArray(item.assets)
    )
      throw Error('invalidResponse');
    const candidate = version(item.tag_name);
    if (item.draft || item.prerelease || !item.tag_name.startsWith('v') || !candidate) continue;
    const v = item.tag_name.replace(/^v/, '');
    if (
      platform !== 'darwin' ||
      !item.assets.some(
        (a: { name?: string; state?: string }) =>
          a.name === `VigilShell-${v}-macos-${arch}.dmg` && a.state === 'uploaded',
      )
    )
      continue;
    if (item.body != null && typeof item.body !== 'string') throw Error('invalidResponse');
    releases.push({ version: v, tag: item.tag_name, notes: (item.body ?? '').slice(0, 65536) });
  }
  releases.sort((a, b) => compare(version(b.version)!, version(a.version)!));
  const newest = releases[0];
  const assets = newest ? data.find((item) => item.tag_name === newest.tag)?.assets : [];
  const downloadable =
    !!newest &&
    [`VigilShell-${newest.version}-macos-${arch}.zip`, 'latest-mac.yml'].every((name) =>
      assets?.some(
        (asset: { name?: string; state?: string }) =>
          asset.name === name && asset.state === 'uploaded',
      ),
    );
  return {
    canDownload: downloadable,
    phase: !newest
      ? 'empty'
      : compare(version(newest.version)!, installed) > 0
        ? 'available'
        : 'current',
    release: newest && compare(version(newest.version)!, installed) > 0 ? newest : null,
  };
}
