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
    const selection = JSON.parse(readFileSync(`${input}/selection.json`, 'utf8'));
    assert.equal(selection.sha, git('rev-parse', 'HEAD'));
    assert.equal(selection.runId, process.env.GITHUB_RUN_ID);
    assert.equal(selection.aggregationAttempt, process.env.GITHUB_RUN_ATTEMPT);
    assert.deepEqual(Object.keys(selection.artifacts).sort(), ['arm64', 'x64']);
    const attempts = {};
    for (const arch of ['arm64', 'x64']) {
      const item = selection.artifacts[arch];
      assert(Number.isSafeInteger(item.id) && item.id > 0);
      assert.equal(
        item.name,
        `preview-macos-${arch}-${selection.sha}-${selection.runId}-${item.attempt}`,
      );
      attempts[arch] = item.attempt;
    }
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
      attempts,
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
          aggregation: selection,
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
