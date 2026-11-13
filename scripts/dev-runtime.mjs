import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { createConnection } from 'node:net';

/** Hash deployable bytes, not generation paths or signing timestamps. */
export async function runtimeFingerprint(root, generation) {
  const hash = createHash('sha256');
  async function walk(directory, prefix) {
    for (const entry of (await readdir(directory, { withFileTypes: true })).sort((a, b) =>
      a.name.localeCompare(b.name),
    )) {
      const path = join(directory, entry.name);
      const name = `${prefix}/${entry.name}`;
      if (entry.isDirectory()) await walk(path, name);
      else if (entry.isFile() && !name.endsWith('.map') && entry.name !== 'manifest.json') {
        hash.update(name).update(await readFile(path));
      }
    }
  }
  await walk(join(root, 'out/main'), 'main');
  await walk(join(root, 'out/preload'), 'preload');
  hash.update(await nativeFingerprint(join(root, '.cache/vigilshell-dev/native', generation)));
  return hash.digest('hex');
}

export function notifyRuntime(message) {
  const path = process.env.VIGILSHELL_DEV_CONTROL;
  if (!path) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const socket = createConnection(path);
    socket.once('error', reject);
    socket.once('connect', () => socket.end(JSON.stringify(message)));
    socket.once('close', (error) => {
      if (!error) resolve();
    });
  });
}

/** Only the launcher owns restart decisions, including while system authorization waits. */
export class RuntimeCoordinator {
  constructor({ fingerprint, replace, delay = 400, report = console.log }) {
    this.fingerprint = fingerprint;
    this.replace = replace;
    this.delay = delay;
    this.report = report;
    this.pending = null;
    this.version = 0;
    this.building = false;
    this.authorizing = false;
    this.running = false;
    this.closed = false;
    this.current = null;
  }
  request(value) {
    this.pending = value;
    this.version++;
    this.schedule();
  }
  build(value, success = true) {
    this.building = value || !success;
    this.version++;
    this.schedule();
  }
  authorization(value) {
    this.authorizing = value;
    this.version++;
    this.schedule();
  }
  schedule() {
    clearTimeout(this.timer);
    if (!this.closed && this.pending && !this.running && !this.building && !this.authorizing)
      this.timer = setTimeout(() => void this.flush(), this.delay);
  }
  async flush() {
    if (this.closed || this.running || this.building || this.authorizing || !this.pending) return;
    this.running = true;
    const version = this.version;
    const value = this.pending;
    try {
      const fingerprint = await this.fingerprint(value);
      if (this.closed || this.version !== version || this.building || this.authorizing) return;
      if (fingerprint === this.current) {
        this.pending = null;
        this.report('[dev:app] Unchanged runtime; keeping Main/Backend');
        return;
      }
      this.pending = null;
      const applied = await this.replace(value);
      this.current = typeof applied === 'string' ? applied : fingerprint;
    } catch (error) {
      if (version === this.version) this.pending = null;
      this.report(error);
    } finally {
      this.running = false;
      this.schedule();
    }
  }
  close() {
    this.closed = true;
    clearTimeout(this.timer);
  }
}

export async function nativeFingerprint(directory) {
  const entries = (await readdir(directory)).filter((name) => name.endsWith('.app')).sort();
  const hash = createHash('sha256');
  for (const name of entries) {
    await promisify(execFile)('/usr/bin/codesign', [
      '--verify',
      '--deep',
      '--strict',
      join(directory, name),
    ]);
    const { stdout, stderr } = await promisify(execFile)('/usr/bin/codesign', [
      '-dvvv',
      '-r-',
      join(directory, name),
    ]);
    const details = stdout + stderr;
    const cdhash = details.match(/^CDHash=(.+)$/m)?.[1];
    const requirement = details.match(/^designated => (.+)$/m)?.[1];
    if (!cdhash || !requirement) throw Error('Missing signed native identity');
    hash.update(name).update(cdhash).update(requirement);
  }
  if (!entries.length) throw Error('Missing native bundles');
  return hash.digest('hex');
}

export function vaultReadOutcome(line) {
  try {
    const fields = JSON.parse(line).fields;
    if (fields?.application !== 'vigilshell') return null;
    if (fields.stage === 'vault_initialization' && fields.outcome === 'finished') return false;
    if (fields.stage !== 'vault_key_read') return null;
    if (fields.outcome === 'started') return true;
    return ['ok', 'failed'].includes(fields.outcome) ? false : null;
  } catch {
    return null;
  }
}
