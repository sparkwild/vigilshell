import { describe, it, expect } from 'vitest';
import { resolveLocale } from './locale';
describe('system language preferences', () => {
  it.each([
    [['fr-FR', 'zh-TW', 'en-US'], 'zh-CN'],
    [['en-GB', 'zh-CN'], 'en'],
    [['ja-JP', 'de-DE'], 'en'],
    [['zh_Hant'], 'zh-CN'],
    [[], 'en'],
  ] as const)('matches preferred supported language %j', (languages, expected) =>
    expect(resolveLocale('system', languages)).toBe(expected),
  );
  it('keeps fixed choices', () => {
    expect(resolveLocale('en', ['zh-CN'])).toBe('en');
    expect(resolveLocale('zh-CN', ['en'])).toBe('zh-CN');
  });
});
