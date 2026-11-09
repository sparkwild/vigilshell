import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

// Each command owns its process group so a stuck security helper cannot hold cleanup open.
export function runCleanupCommand(command, args, timeoutMs = 15000) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { detached: true, stdio: ['ignore', 'inherit', 'inherit'] });
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      try {
        if (command === 'sudo') {
          execFileSync('/usr/bin/sudo', ['-n', '/bin/kill', '-KILL', '--', `-${child.pid}`], {
            timeout: 2000,
            killSignal: 'SIGKILL',
            stdio: ['ignore', 'inherit', 'inherit'],
          });
        } else {
          process.kill(-child.pid, 'SIGKILL');
        }
      } catch (error) {
        if (error.code !== 'ESRCH')
          console.error('Could not terminate cleanup group:', error.message);
      }
      child.unref();
      reject(new Error(`${command} timed out after ${timeoutMs} ms`));
    }, timeoutMs);
    child.once('error', (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once('exit', (code, signal) => {
      clearTimeout(timer);
      if (timedOut) return;
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with ${code ?? signal}`));
    });
  });
}

export async function cleanupSigning(root, evidence, run = runCleanupCommand) {
  const stages = [];
  async function stage(name, operation) {
    console.log(`Signing cleanup: ${name} started`);
    try {
      await operation();
      stages.push({ name, passed: true });
      console.log(`Signing cleanup: ${name} passed`);
    } catch (error) {
      stages.push({ name, passed: false, error: error.message });
      console.error(`Signing cleanup: ${name} failed: ${error.message}`);
    }
  }
  if (existsSync(root)) {
    await stage('remove exported private keys', async () => {
      for (const name of ['key.pem', 'identity.p12']) await rm(join(root, name), { force: true });
    });
    const certificate = join(root, 'certificate.pem');
    if (existsSync(certificate)) {
      await stage('remove administrator certificate trust', () =>
        run('sudo', ['-n', 'security', 'remove-trusted-cert', '-d', certificate]),
      );
    }
    const keychain = join(root, 'ci.keychain-db');
    if (existsSync(keychain)) {
      await stage('delete temporary keychain', () =>
        run('security', ['delete-keychain', keychain]),
      );
    }
    const previous = join(root, 'keychains-before.txt');
    if (existsSync(previous)) {
      await stage('restore and verify keychain search list', () =>
        run('python3', [
          '-c',
          `import pathlib, shlex, subprocess, sys
expected = shlex.split(pathlib.Path(sys.argv[1]).read_text())
subprocess.run(['security', 'list-keychains', '-d', 'user', '-s', *expected], check=True)
actual = shlex.split(subprocess.check_output(['security', 'list-keychains', '-d', 'user'], text=True))
if actual != expected: raise RuntimeError('Keychain search list was not restored')`,
          previous,
        ]),
      );
    }
    await stage('remove temporary signing files', async () => {
      await rm(root, { recursive: true, force: true });
      assert(!existsSync(root), 'Signing directory still exists');
    });
  }
  const passed = stages.every((stage) => stage.passed);
  await mkdir(evidence, { recursive: true });
  await writeFile(
    join(evidence, 'signing-cleanup.json'),
    JSON.stringify({ passed, stages }, null, 2),
  );
  if (!passed) throw new Error('Signing cleanup incomplete; see signing-cleanup.json');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  assert(
    process.env.GITHUB_ACTIONS === 'true' &&
      process.env.RUNNER_ENVIRONMENT === 'github-hosted' &&
      process.env.RUNNER_OS === 'macOS' &&
      process.platform === 'darwin' &&
      process.env.RUNNER_TEMP,
    'Signing cleanup requires a disposable GitHub-hosted macOS runner',
  );
  await cleanupSigning(
    join(process.env.RUNNER_TEMP, 'vigilshell-ci-signing'),
    join(process.env.RUNNER_TEMP, 'vigilshell-ci-evidence'),
  );
}
