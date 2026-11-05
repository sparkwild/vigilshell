import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { alive, createEvidenceDirectory, stopLauncher, until } from '../helpers/dev-session.mjs';

test('separate runs and attached clients keep earlier evidence intact under the same base', async () => {
  const base = await mkdtemp(join(tmpdir(), 'vigilshell-evidence-'));
  try {
    const first = await createEvidenceDirectory(base, 'lifecycle');
    await writeFile(join(first, 'development.log'), 'original launcher output');
    await writeFile(join(first, 'data-preservation.json'), 'original preservation receipt');
    const [second, attached] = await Promise.all([
      createEvidenceDirectory(base, 'lifecycle'),
      createEvidenceDirectory(base, 'attached'),
    ]);
    assert.equal(new Set([first, second, attached]).size, 3);
    for (const directory of [second, attached]) {
      await writeFile(join(directory, 'development.log'), '');
      await writeFile(join(directory, 'data-preservation.json'), 'later receipt');
    }
    assert.equal(
      await readFile(join(first, 'development.log'), 'utf8'),
      'original launcher output',
    );
    assert.equal(
      await readFile(join(first, 'data-preservation.json'), 'utf8'),
      'original preservation receipt',
    );
  } finally {
    await rm(base, { recursive: true, force: true });
  }
});

test('failed initialization stops its launcher and a supervised detached child group', async () => {
  const supervisor = `
    const {spawn}=require('node:child_process');
    const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{detached:true,stdio:'ignore'});
    process.on('SIGTERM',()=>child.kill('SIGTERM'));
    child.on('exit',()=>process.exit(0));
    console.log(child.pid);
  `;
  const launcher = spawn(process.execPath, ['-e', supervisor], {
    detached: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const [chunk] = await once(launcher.stdout, 'data');
  const child = Number(chunk.toString().trim());
  assert(child > 0 && alive(child));
  try {
    await stopLauncher(launcher);
    assert(!alive(launcher.pid));
    assert(!alive(child));
  } finally {
    if (alive(child)) process.kill(child, 'SIGTERM');
    if (alive(launcher.pid)) launcher.kill('SIGTERM');
    await until(() => !alive(child) && !alive(launcher.pid), 'Synthetic launcher did not stop');
  }
});

test('an already exited launcher does not prevent failure cleanup', async () => {
  const launcher = spawn(process.execPath, ['-e', 'process.exit(2)'], {
    detached: true,
    stdio: 'ignore',
  });
  await once(launcher, 'exit');
  await stopLauncher(launcher);
  assert.equal(launcher.exitCode, 2);
});
