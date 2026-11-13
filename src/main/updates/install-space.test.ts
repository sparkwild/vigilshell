import { expect, it } from 'vitest';
import { requiredInstallSpace } from './install-space';
it('allows for extraction, replacement staging and native ZIP caching', () => {
  expect(
    requiredInstallSpace(
      '642 files, 366859251 bytes uncompressed, 147814000 bytes compressed: 59.7%',
      147814000,
    ),
  ).toBe(881532502);
});
it.each([
  'invalid ZIP',
  '3 files, 0 bytes uncompressed',
  '3 files, 999999999999999999999 bytes uncompressed',
])('rejects an unusable archive size: %s', (summary) => {
  expect(() => requiredInstallSpace(summary, 100)).toThrow('invalid_archive');
});
