import { expect, it, vi } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveLocale } from './i18n/locale';
import {
  normalizeLocalizedNotes,
  normalizeReleaseNotes,
  parseMarkdownReleaseNotes,
  releaseNotesFromDocument,
  selectReleaseNotes,
} from './release-notes';

it('extracts only the three supported sections and normalizes Markdown to readable text', () => {
  expect(
    parseMarkdownReleaseNotes(
      'Intro\n## Features\n- **Fast** updates with `offline` recovery.\n\n```md\n## Upgrade Notes\nHidden example\n```\n## Bug Fixes\n- Fixed [downloads](https://example.com).\n## Upgrade Notes\nBack up settings.\n## Downloads\n<table>installers</table>\n## Build & Verification\nBuild run',
    ),
  ).toEqual({
    features: ['Fast updates with offline recovery.'],
    fixes: ['Fixed downloads (https://example.com).'],
    upgradeNotes: ['Back up settings.'],
  });
  expect(parseMarkdownReleaseNotes('Unstructured legacy body')).toEqual({});
  expect(
    parseMarkdownReleaseNotes(
      '## constructor\nUnknown\n## Features\n- *Italic* and ~~removed~~\n\n> Quoted copy',
    ),
  ).toEqual({ features: ['Italic and removed', 'Quoted copy'] });
});

const release = {
  version: '0.2.0',
  tag: 'v0.2.0',
  notes: '## Features\n- Legacy feature\n## Downloads\nDo not display',
  localizedNotes: {
    en: { features: ['Feature'], upgradeNotes: ['Back up'] },
    'zh-CN': { features: ['功能'], upgradeNotes: ['备份'] },
  },
};
it('selects by resolved locale and falls back to the whole default-language document', () => {
  expect(selectReleaseNotes(release, 'zh-CN')).toEqual({
    sections: release.localizedNotes['zh-CN'],
    fallback: false,
  });
  expect(selectReleaseNotes(release, 'en').sections).toEqual(release.localizedNotes.en);
  const incomplete = {
    ...release,
    localizedNotes: { ...release.localizedNotes, 'zh-CN': { features: ['功能'] } },
  };
  expect(selectReleaseNotes(incomplete, 'zh-CN')).toEqual({
    sections: release.localizedNotes.en,
    fallback: true,
  });
  expect(selectReleaseNotes({ ...release, localizedNotes: undefined }, 'zh-CN')).toEqual({
    sections: { features: ['Legacy feature'] },
    fallback: true,
  });
});

it.each(['fr', 'ja-JP'])(
  'selects %s notes by locale and falls back for missing, incomplete or invalid translations',
  (locale) => {
    const translation = { features: ['Translated feature'], upgradeNotes: ['Translated notice'] };
    const localized = {
      ...release,
      localizedNotes: { ...release.localizedNotes, [locale]: translation },
    };
    expect(selectReleaseNotes(localized, locale)).toEqual({
      sections: translation,
      fallback: false,
    });
    for (const notes of [undefined, { features: ['Incomplete'] }, { features: [42] }]) {
      expect(
        selectReleaseNotes(
          { ...localized, localizedNotes: { ...release.localizedNotes, [locale]: notes } } as never,
          locale,
        ),
      ).toEqual({ sections: release.localizedNotes.en, fallback: true });
    }
    expect(selectReleaseNotes({ ...release, localizedNotes: undefined }, locale)).toEqual({
      sections: { features: ['Legacy feature'] },
      fallback: true,
    });
  },
);

it('uses the resolved system language while respecting a fixed app language', () => {
  expect(selectReleaseNotes(release, resolveLocale('system', ['zh-TW', 'en-US'])).sections).toEqual(
    release.localizedNotes['zh-CN'],
  );
  expect(selectReleaseNotes(release, resolveLocale('en', ['zh-CN'])).sections).toEqual(
    release.localizedNotes.en,
  );
  expect(selectReleaseNotes(release, resolveLocale('system', ['ja-JP'])).sections).toEqual(
    release.localizedNotes.en,
  );
  expect(selectReleaseNotes(release, 'constructor')).toEqual({
    sections: release.localizedNotes.en,
    fallback: true,
  });
  expect(selectReleaseNotes({ ...release, localizedNotes: undefined, notes: '' }, 'fr')).toEqual({
    sections: {},
    fallback: false,
  });
});

it('retains complete locale documents independently and requires valid default-language notes', () => {
  const french = { features: ['Fonction'], upgradeNotes: ['Sauvegarder'] };
  expect(
    normalizeLocalizedNotes({
      ...release.localizedNotes,
      fr: french,
      de: { features: ['Partial'] },
    }),
  ).toEqual({ ...release.localizedNotes, fr: french });
  expect(normalizeLocalizedNotes({ fr: french })).toBeUndefined();
});

it('uses configured locales and the configured default for selection and publishing', async () => {
  const root = await mkdtemp(join(tmpdir(), 'vs-note-locales-'));
  vi.resetModules();
  vi.doMock('../../project.inlang/settings.json', () => ({
    default: { baseLocale: 'fr', locales: ['fr', 'en', 'zh-CN'] },
  }));
  try {
    const { selectReleaseNotes: select, defaultReleaseNotesLocale } =
      await import('./release-notes');
    const french = { features: ['Fonction'], upgradeNotes: ['Sauvegarder'] };
    const localized = { ...release, localizedNotes: { ...release.localizedNotes, fr: french } };
    expect(defaultReleaseNotesLocale).toBe('fr');
    expect(select(localized, 'fr')).toEqual({ sections: french, fallback: false });
    expect(select(localized, 'en')).toEqual({
      sections: release.localizedNotes.en,
      fallback: false,
    });
    expect(select(localized, 'de')).toEqual({ sections: french, fallback: true });
    expect(select({ ...localized, localizedNotes: { fr: french } }, 'zh-CN')).toEqual({
      sections: french,
      fallback: true,
    });
    const directory = join(root, 'docs/releases/v0.2.0');
    await mkdir(directory, { recursive: true });
    await writeFile(
      join(directory, 'fr.md'),
      '## Features\n\n- Fonction\n\n## Upgrade Notes\n\n- Sauvegarder\n',
    );
    await writeFile(
      join(directory, 'en.md'),
      '## Features\n\n- Feature\n\n## Upgrade Notes\n\n- Back up\n',
    );
    const { createReleaseNotesDocument, readReleaseNotes } =
      await import('../../scripts/release-notes.mjs');
    const document = createReleaseNotesDocument('v0.2.0', root);
    expect(document.languages).toEqual({ en: release.localizedNotes.en, fr: french });
    expect(readReleaseNotes('v0.2.0', root)).toMatchObject({ Features: '- Fonction' });
    await rm(join(directory, 'fr.md'));
    expect(() => createReleaseNotesDocument('v0.2.0', root)).toThrow('fr.md');
  } finally {
    vi.doUnmock('../../project.inlang/settings.json');
    vi.resetModules();
    await rm(root, { recursive: true, force: true });
  }
});

it('binds optional documents to their version and tag and rejects unsupported or oversized data', () => {
  const document = {
    schema: 1,
    version: release.version,
    tag: release.tag,
    languages: release.localizedNotes,
  };
  expect(releaseNotesFromDocument(document, '0.2.0', 'v0.2.0')).toEqual(release.localizedNotes);
  for (const patch of [
    { schema: 2 },
    { version: '0.3.0' },
    { tag: 'v0.3.0' },
    { languages: { en: { downloads: ['No'] } } },
  ])
    expect(releaseNotesFromDocument({ ...document, ...patch }, '0.2.0', 'v0.2.0')).toBeUndefined();
  expect(
    releaseNotesFromDocument(
      { ...document, languages: { en: { features: ['x'.repeat(65537)] } } },
      '0.2.0',
      'v0.2.0',
    ),
  ).toBeUndefined();
  expect(normalizeReleaseNotes({ ...release, notes: null, localizedNotes: {} } as never)).toEqual({
    version: release.version,
    tag: release.tag,
    notes: '',
  });
});
