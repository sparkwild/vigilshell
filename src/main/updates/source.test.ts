import { expect, it } from 'vitest';
import { updateSource } from './source';
it('never accepts development feed overrides in a release build', () => {
  const source = updateSource(false, { VIGILSHELL_DEV_UPDATE_FEED: 'http://127.0.0.1:1234/test' });
  expect(source.downloadable).toBe(true);
  expect(source.feed('v1.2.3')).toBe(
    'https://github.com/sparkwild/vigilshell/releases/download/v1.2.3/',
  );
  expect(updateSource(true, {}).downloadable).toBe(false);
});
it('isolates Dev fixtures to loopback HTTP feeds', () => {
  for (const url of ['https://example.com', 'http://example.com', 'http://user:secret@127.0.0.1'])
    expect(() => updateSource(true, { VIGILSHELL_DEV_UPDATE_FEED: url })).toThrow();
  expect(
    updateSource(true, { VIGILSHELL_DEV_UPDATE_FEED: 'http://127.0.0.1:1234/test' }).releases,
  ).toBe('http://127.0.0.1:1234/test/releases');
});
