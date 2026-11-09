import { describe, expect, it } from 'vitest';
import type { RemoteEntry } from '@contracts';
import {
  initialPermissionChoices,
  parsePermissionMode,
  permissionMasks,
  permissionMode,
} from './file-permissions';
const entry = (permissions: number) => ({ permissions }) as RemoteEntry;
describe('file permission masks', () => {
  it('changes only edited bits for a single item and preserves special bits outside the mask', () => {
    const original = initialPermissionChoices([entry(0o4755)]),
      next = [...original];
    next[4] = 'allow';
    expect(permissionMode(original)).toBe('755');
    expect(permissionMasks(next, original, false)).toEqual({ setMask: 0o020, clearMask: 0 });
  });
  it('leaves all bits unchanged for a multiple selection until explicitly chosen', () => {
    const choices = initialPermissionChoices([entry(0o600), entry(0o755)]);
    expect(permissionMode(choices)).toBe('');
    expect(permissionMasks(choices, choices, true)).toEqual({ setMask: 0, clearMask: 0 });
    const next = [...choices];
    next[0] = 'allow';
    next[8] = 'deny';
    expect(permissionMasks(next, choices, true)).toEqual({ setMask: 0o400, clearMask: 0o001 });
  });
  it('octal input explicitly defines all nine bits and rejects invalid or special-bit modes', () => {
    const original = initialPermissionChoices([entry(0o600)]),
      next = parsePermissionMode('640')!;
    expect(permissionMasks(next, original, true)).toEqual({ setMask: 0o640, clearMask: 0o137 });
    for (const mode of ['64', '888', '4755', '-01', ' 600', '600 '])
      expect(parsePermissionMode(mode)).toBeNull();
  });
});
