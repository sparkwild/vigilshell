import type { RemoteEntry } from '@contracts';

export type PermissionChoice = 'keep' | 'allow' | 'deny';
export const permissionBits = [0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001];
export function initialPermissionChoices(entries: RemoteEntry[]): PermissionChoice[] {
  if (entries.length !== 1) return permissionBits.map(() => 'keep');
  return permissionBits.map((bit) => ((entries[0]!.permissions ?? 0) & bit ? 'allow' : 'deny'));
}
export function permissionMasks(
  choices: PermissionChoice[],
  original: PermissionChoice[],
  allBits: boolean,
) {
  let setMask = 0,
    clearMask = 0;
  for (const [index, bit] of permissionBits.entries()) {
    const choice = choices[index];
    if (!allBits && original[index] === choice) continue;
    if (choice === 'allow') setMask |= bit;
    if (choice === 'deny') clearMask |= bit;
  }
  return { setMask, clearMask };
}
export function parsePermissionMode(value: string): PermissionChoice[] | null {
  if (!/^[0-7]{3}$/.test(value)) return null;
  const mode = parseInt(value, 8);
  return permissionBits.map((bit) => (mode & bit ? 'allow' : 'deny'));
}
export function permissionMode(choices: PermissionChoice[]) {
  if (choices.includes('keep')) return '';
  const mode = choices.reduce(
    (value, choice, index) => value | (choice === 'allow' ? permissionBits[index]! : 0),
    0,
  );
  return mode.toString(8).padStart(3, '0');
}
