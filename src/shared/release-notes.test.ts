import { expect, it } from 'vitest';
import {
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
it('selects by resolved locale and falls back to the whole English document', () => {
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
