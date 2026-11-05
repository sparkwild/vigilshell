import type { LocaleMode, ResolvedLocale } from '@contracts';
/** main 根据系统偏好解析；固定选择不会被系统更新覆盖。 */
export function resolveLocale(mode: LocaleMode, languages: readonly string[]): ResolvedLocale {
  if (mode !== 'system') return mode;
  for (const language of languages) {
    const prefix = language.toLowerCase().split(/[-_]/)[0];
    if (prefix === 'zh') return 'zh-CN';
    if (prefix === 'en') return 'en';
  }
  return 'en';
}
