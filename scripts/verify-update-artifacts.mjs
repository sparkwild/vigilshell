import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { basename, join } from 'node:path';

const { load } = createRequire(import.meta.url)('js-yaml');
export function inspectUpdateArtifacts(directory, version, arch) {
  const metadata = load(readFileSync(join(directory, 'latest-mac.yml'), 'utf8'));
  assert.equal(metadata.version, version);
  assert.equal(
    metadata.files.length,
    1,
    'macOS updater metadata must reference only the final ZIP',
  );
  const file = metadata.files[0];
  assert.equal(file.url, `VigilShell-${version}-macos-${arch}.zip`);
  const archive = join(directory, file.url);
  const bytes = readFileSync(archive);
  assert.equal(file.size, bytes.length);
  assert.equal(file.sha512, createHash('sha512').update(bytes).digest('base64'));
  return {
    archive,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    metadataSha256: createHash('sha256')
      .update(readFileSync(join(directory, 'latest-mac.yml')))
      .digest('hex'),
  };
}
export async function verifyUpdateArtifacts(directory, name, verify) {
  const { version } = JSON.parse(readFileSync('package.json', 'utf8'));
  const { archive, sha256 } = inspectUpdateArtifacts(directory, version, process.arch);
  await writeFile(`${archive}.sha256`, `${sha256}  ${basename(archive)}\n`);
  const unpacked = await mkdtemp(join(directory, '.verify-zip-'));
  try {
    execFileSync('/usr/bin/ditto', ['-x', '-k', archive, unpacked]);
    assert(
      readFileSync(join(unpacked, 'LICENSES.chromium.html')).equals(
        readFileSync('node_modules/electron/dist/LICENSES.chromium.html'),
      ),
    );
    const app = join(unpacked, `${name}.app`);
    const inspect = `import pathlib,stat,sys\np=pathlib.Path(sys.argv[1])\nassert all(x.is_symlink() or x.stat().st_mode & stat.S_IWUSR for x in p.rglob('*')), 'Read-only update resources'\n`;
    execFileSync('python3', ['-c', inspect, app]);
    await verify(app);
  } finally {
    await rm(unpacked, { recursive: true, force: true });
  }
  console.log(
    'Verified final update ZIP, metadata digest, writable resources, notices and signatures',
  );
}
