import type { AvailableRelease, ReleaseNoteSections } from '../contracts';
import localeSettings from '../../project.inlang/settings.json' with { type: 'json' };

type LocalizedReleaseNotes = NonNullable<AvailableRelease['localizedNotes']>;

export const releaseNoteSections = ['features', 'fixes', 'upgradeNotes'] as const;
export const releaseNotesLimit = 64 * 1024;
export const defaultReleaseNotesLocale = localeSettings.baseLocale;
export const releaseNotesLocales = localeSettings.locales;
const headings: Record<string, (typeof releaseNoteSections)[number]> = {
  Features: 'features',
  'Bug Fixes': 'fixes',
  'Upgrade Notes': 'upgradeNotes',
  新增功能: 'features',
  问题修复: 'fixes',
  升级注意事项: 'upgradeNotes',
};

function plainText(value: string) {
  return value
    .replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, '')
    .replace(/<[^>]*>/g, '')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1 ($2)')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*|__([^_]+)__/g, (_, a, b) => a ?? b)
    .replace(
      /(?<!\*)\*([^*\n]+)\*(?!\*)|(?<!\w)_([^_\n]+)_(?!\w)|~~([^~\n]+)~~/g,
      (_, a, b, c) => a ?? b ?? c,
    )
    .replace(/^\s*>\s?/gm, '')
    .replace(/^#{1,6}\s+/gm, '')
    .trim();
}

/** Extract only user-facing sections; generated download and build sections are excluded. */
export function parseMarkdownReleaseNotes(markdown: string): ReleaseNoteSections {
  const result: ReleaseNoteSections = {};
  let section: (typeof releaseNoteSections)[number] | undefined;
  let paragraph: string[] = [];
  let fence: string | undefined;
  const flush = () => {
    const text = plainText(paragraph.join('\n'));
    if (section && text) (result[section] ??= []).push(text);
    paragraph = [];
  };
  for (const line of markdown.slice(0, releaseNotesLimit).replaceAll('\r\n', '\n').split('\n')) {
    const delimiter = /^\s*(`{3,}|~{3,})/.exec(line)?.[1];
    if (delimiter) {
      if (!fence) {
        flush();
        fence = delimiter[0];
      } else if (delimiter[0] === fence) fence = undefined;
      continue;
    }
    if (fence) continue;
    const heading = /^(#{1,2})\s+(.+?)\s*#*\s*$/.exec(line);
    if (heading) {
      flush();
      section =
        heading[1] === '##' && Object.hasOwn(headings, heading[2]!)
          ? headings[heading[2]!]
          : undefined;
      continue;
    }
    if (!section) continue;
    if (!line.trim()) {
      flush();
      continue;
    }
    const item = /^\s*(?:[-*+] |\d+[.)] )(.+)$/.exec(line);
    if (item) {
      flush();
      paragraph.push(item[1]!);
    } else paragraph.push(line.trim());
  }
  flush();
  return result;
}

function sections(value: unknown): ReleaseNoteSections | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return;
  if (Object.keys(value).some((key) => !releaseNoteSections.includes(key as never))) return;
  const result: ReleaseNoteSections = {};
  for (const key of releaseNoteSections) {
    const items = (value as ReleaseNoteSections)[key];
    if (items === undefined) continue;
    if (
      !Array.isArray(items) ||
      !items.length ||
      items.length > 128 ||
      items.some((text) => typeof text !== 'string' || !text.trim() || text.length > 8192)
    )
      return;
    result[key] = items.map((text) => plainText(text)).filter(Boolean);
    if (!result[key]!.length) return;
  }
  return Object.keys(result).length ? result : undefined;
}

/** Translation coverage is all-or-nothing so upgrade notices cannot disappear on locale changes. */
export function normalizeLocalizedNotes(value: unknown): LocalizedReleaseNotes | undefined {
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    JSON.stringify(value).length > releaseNotesLimit
  )
    return;
  const languages = value as Record<string, unknown>;
  const defaultNotes = Object.hasOwn(languages, defaultReleaseNotesLocale)
    ? sections(languages[defaultReleaseNotesLocale])
    : undefined;
  if (!defaultNotes) return;
  const result = new Map([[defaultReleaseNotesLocale, defaultNotes]]);
  for (const [locale, content] of Object.entries(languages)) {
    if (locale === defaultReleaseNotesLocale) continue;
    const translated = sections(content);
    if (translated && releaseNoteSections.every((key) => !!defaultNotes[key] === !!translated[key]))
      result.set(locale, translated);
  }
  return Object.fromEntries(result);
}

export function releaseNotesFromDocument(value: unknown, version: string, tag: string) {
  if (!value || typeof value !== 'object') return;
  const doc = value as { schema?: unknown; version?: unknown; tag?: unknown; languages?: unknown };
  if (doc.schema !== 1 || doc.version !== version || doc.tag !== tag) return;
  return normalizeLocalizedNotes(doc.languages);
}

export function normalizeReleaseNotes(release: AvailableRelease): AvailableRelease {
  const notes = typeof release.notes === 'string' ? release.notes.slice(0, releaseNotesLimit) : '';
  const localized = normalizeLocalizedNotes(release.localizedNotes);
  const result = { ...release, notes };
  delete result.localizedNotes;
  return { ...result, ...(localized ? { localizedNotes: localized } : {}) };
}

export function selectReleaseNotes(release: AvailableRelease, locale: string) {
  const localized = normalizeLocalizedNotes(release.localizedNotes);
  const defaultNotes =
    localized?.[defaultReleaseNotesLocale] ?? parseMarkdownReleaseNotes(release.notes ?? '');
  const translated = localized && Object.hasOwn(localized, locale) ? localized[locale] : undefined;
  return {
    sections: translated ?? defaultNotes,
    fallback:
      locale !== defaultReleaseNotesLocale && !translated && Object.keys(defaultNotes).length > 0,
  };
}
