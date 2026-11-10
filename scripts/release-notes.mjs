import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const headings = ['Features', 'Bug Fixes', 'Upgrade Notes'];

export function readReleaseNotes(tag, root = '.') {
  const name = `docs/releases/${tag}.md`;
  const path = resolve(root, name);
  assert(existsSync(path), `Missing release notes: ${name}`);
  const [intro, ...blocks] = readFileSync(path, 'utf8')
    .replaceAll('\r\n', '\n')
    .trim()
    .split(/^## (.+)$/m);
  assert(!intro.trim(), `${name} must start with a release notes heading`);

  const notes = {};
  for (let index = 0; index < blocks.length; index += 2) {
    const heading = blocks[index];
    assert(headings.includes(heading), `${name} has an unsupported section: ${heading}`);
    assert(!notes[heading], `${name} repeats the ${heading} section`);
    const content = blocks[index + 1].trim();
    assert(content, `${name} has an empty ${heading} section`);
    notes[heading] = content;
  }
  assert(Object.keys(notes).length, `${name} requires at least one release notes section`);
  return notes;
}

export function renderNoteSections(notes, heading = '##') {
  return headings
    .filter((title) => notes[title])
    .map((title) => `${heading} ${title}\n\n${notes[title]}`);
}

const escapeAttribute = (value) =>
  value
    .replaceAll('&', '&amp;')
    .replaceAll('"', '&quot;')
    .replaceAll('<', '&lt;')
    .replaceAll('|', '&#124;');

function renderDownloadTable(installers, assetUrl) {
  const rows = ['| OS | Download |', '| :--- | :--- |'];
  for (const [os, packages] of Map.groupBy(installers, (installer) => installer.os)) {
    const badges = packages.map(({ format, architecture, color, logo, name }) => {
      const query = new URLSearchParams({
        label: format,
        message: architecture,
        color,
        logo,
        logoColor: 'white',
        style: 'flat',
      });
      const badgeUrl = escapeAttribute(`https://img.shields.io/static/v1?${query}`);
      const alt = escapeAttribute(`${format} for ${architecture}`);
      return `<a href="${assetUrl(name)}"><img src="${badgeUrl}" alt="${alt}" align="absmiddle"></a>`;
    });
    rows.push(`| ${os} | ${badges.join('<br>')} |`);
  }
  return rows.join('\n');
}

export function renderReleaseNotes(
  notes,
  { repository, source, assets, runUrl, previousTag, showChangelog = true },
) {
  const { tag, sha } = source;
  const repoUrl = `https://github.com/${repository}`;
  const downloadUrl = `${repoUrl}/releases/download/${tag}`;
  const assetUrl = (name) => `${downloadUrl}/${encodeURIComponent(name)}`;
  const link = (label, name) => `[${label}](${assetUrl(name)})`;
  assert(assets.installers.length, 'Release downloads require a verified installer');
  const downloads = ['## Downloads', renderDownloadTable(assets.installers, assetUrl)];
  if (showChangelog)
    downloads.push(`**List of all changes** · [Changelog](${repoUrl}/blob/main/CHANGELOG.md)`);
  const installation = [
    'Open the DMG and drag `VigilShell.app` to Applications. To upgrade, quit VigilShell before replacing the app.',
    "The macOS installer uses VigilShell's fixed self-signed certificate without Apple notarization. If macOS blocks the first launch, verify the source and use **Privacy & Security → Open Anyway** for VigilShell.",
  ].join('\n\n');
  const build = [
    `- Source commit [\`${sha.slice(0, 7)}\`](${repoUrl}/commit/${sha})`,
    `- [Build run](${runUrl}) · ${link('Build manifest', assets.manifest)} · ${link('Signing report', assets.signingReport)}`,
  ];
  if (previousTag)
    build.push(`- [Compare source changes](${repoUrl}/compare/${previousTag}...${tag})`);

  return [
    ...renderNoteSections(notes),
    ...downloads,
    `## Installation & Upgrade\n\n${installation}`,
    `## Build & Verification\n\n${build.join('\n')}`,
  ].join('\n\n');
}
