import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { validateAssets, validateDraft } from './release-assets.mjs';
import { verifySource } from './release-policy.mjs';

const { mode, version, sha } = verifySource();
assert.equal(mode, 'publish', 'Manual verification cannot publish');
const tag = `v${version}`;
const { dmg, checksum, report: reportPath } = validateAssets('release-assets', version, sha);
const repo = 'sparkwild/vigilshell';
const gh = (...args) => execFileSync('gh', args, { encoding: 'utf8' }).trim();
const releases = JSON.parse(gh('api', `repos/${repo}/releases`, '--paginate', '--slurp')).flat();
let release = releases.find((item) => item.tag_name === tag);
if (release) {
  validateDraft(release, sha);
} else {
  const body = {
    tag_name: tag,
    target_commitish: sha,
    name: `VigilShell ${tag}`,
    draft: true,
    prerelease: true,
    make_latest: 'false',
    body: `macOS ARM64 Beta. Fixed self-signed certificate; not notarized.\n\nSource: ${sha}\n\nDownload the DMG and verify its SHA-256 before installation. See the repository installation instructions. No automatic updater is included.`,
  };
  release = JSON.parse(
    execFileSync('gh', ['api', `repos/${repo}/releases`, '--method', 'POST', '--input', '-'], {
      input: JSON.stringify(body),
      encoding: 'utf8',
    }),
  );
}
gh('release', 'upload', tag, dmg, checksum, reportPath, '--repo', repo, '--clobber');
const remote = JSON.parse(gh('api', `repos/${repo}/releases/${release.id}`));
assert(remote.draft);
assert.equal(remote.assets.length, 3);
// Verify the uploaded bytes, not just asset names and lengths.
for (const path of [dmg, checksum, reportPath]) {
  const filename = path.split('/').at(-1);
  const asset = remote.assets.find((item) => item.name === filename);
  assert(asset, `Missing uploaded asset ${filename}`);
  const bytes = execFileSync(
    'gh',
    ['api', `repos/${repo}/releases/assets/${asset.id}`, '-H', 'Accept: application/octet-stream'],
    { maxBuffer: 1024 * 1024 * 1024 },
  );
  assert.equal(
    createHash('sha256').update(bytes).digest('hex'),
    createHash('sha256').update(readFileSync(path)).digest('hex'),
  );
}
verifySource();
execFileSync(
  'gh',
  ['api', `repos/${repo}/releases/${release.id}`, '--method', 'PATCH', '--input', '-'],
  {
    input: JSON.stringify({ draft: false, prerelease: true, make_latest: 'false' }),
    stdio: ['pipe', 'ignore', 'inherit'],
  },
);
console.log(`Published ${tag} from ${sha}`);
