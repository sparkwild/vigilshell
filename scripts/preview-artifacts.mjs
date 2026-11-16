import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkVersions } from './release-policy.mjs';

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();
  if (process.argv[2] === 'capture') {
    assert.equal(git('status', '--porcelain'), '', 'Preview requires a clean checkout');
    const sha = git('rev-parse', 'HEAD');
    mkdirSync('.cache', { recursive: true });
    writeFileSync(
      '.cache/preview-source.json',
      JSON.stringify({ sha, tree: git('rev-parse', 'HEAD^{tree}'), version: checkVersions() }),
    );
    if (process.env.GITHUB_OUTPUT)
      writeFileSync(process.env.GITHUB_OUTPUT, `sha=${sha}\n`, { flag: 'a' });
  } else if (process.argv[2] === 'merge') {
    const { aggregateReleaseBuilds } = await import('./release-artifacts.mjs');
    const input = '.cache/preview-builds';
    const build = JSON.parse(readFileSync(`${input}/arm64/build.json`, 'utf8'));
    const source = { sha: build.sha, tree: build.tree, version: build.version };
    assert.equal(git('rev-parse', 'HEAD'), source.sha);
    assert.equal(git('rev-parse', 'HEAD^{tree}'), source.tree);
    const assembled = aggregateReleaseBuilds(
      input,
      '.cache/preview-assembled',
      source,
      undefined,
      process.env.GITHUB_RUN_ID,
      process.env.GITHUB_RUN_ATTEMPT,
      true,
    );
    mkdirSync('preview', { recursive: true });
    for (const file of ['latest-mac.yml', 'signing-report.json'])
      writeFileSync(`preview/${file}`, readFileSync(`.cache/preview-assembled/${file}`));
    writeFileSync(
      'preview/build.json',
      JSON.stringify(
        {
          ...source,
          preview: true,
          architectures: assembled.builds,
          metadataSha256: assembled.metadataSha256,
        },
        null,
        2,
      ) + '\n',
    );
  } else {
    assert.equal(process.argv[2], 'collect');
    const source = JSON.parse(readFileSync('.cache/preview-source.json', 'utf8'));
    assert.equal(git('rev-parse', 'HEAD'), source.sha);
    assert.equal(git('status', '--porcelain'), '', 'Build changed tracked source');
    const { collectReleaseBuild } = await import('./release-artifacts.mjs');
    collectReleaseBuild(source, process.env, true);
  }
}
