import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

// This is a target architecture gate, not a compatibility allowlist. During
// migration it reports remaining work instead of blessing existing violations.
const root = resolve('.');
const manifest = JSON.parse(readFileSync('package.json', 'utf8'));
const app = manifest.name;
const failures = [];
const sharedDependencies = JSON.parse(readFileSync('scripts/foundation-dependencies.json', 'utf8'));
for (const [name, version] of Object.entries(sharedDependencies)) {
  const declared = manifest.dependencies?.[name] ?? manifest.devDependencies?.[name];
  if (declared !== version)
    failures.push(`${name}: expected shared baseline ${version}, received ${declared}`);
}

if (manifest.workspaces) failures.push('single desktop application must not use npm workspaces');
for (const path of ['apps/desktop/package.json', 'packages/contracts/package.json']) {
  if (existsSync(path)) failures.push(`retired application layout: ${path}`);
}
/** @type {{ packages: Array<{ id: string, name: string, dependencies: Array<{name: string, kind: string | null}> }>, workspace_members: string[] }} */
const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], { encoding: 'utf8' }),
);
const local = new Map(
  metadata.packages
    .filter((p) => metadata.workspace_members.includes(p.id))
    .map((p) => [p.name, p]),
);
for (const role of [
  'domain',
  'application',
  'desktop-protocol',
  'backend',
  'storage-sqlite',
  'credentials',
]) {
  if (!local.has(`${app}-${role}`)) failures.push(`missing responsibility crate: ${app}-${role}`);
}
for (const pkg of local.values()) {
  const role = pkg.name.slice(app.length + 1);
  for (const dep of pkg.dependencies.filter((d) => d.kind !== 'dev')) {
    const localRole = dep.name.startsWith(`${app}-`) ? dep.name.slice(app.length + 1) : null;
    if (
      role === 'domain' &&
      (localRole ||
        /^(tokio|rusqlite|mysql_async|redis|russh|security-framework|objc2)/.test(dep.name))
    ) {
      failures.push(`${pkg.name} must not depend on ${dep.name}`);
    }
    if (
      role === 'application' &&
      /^(rusqlite|mysql_async|redis|russh|security-framework|objc2)/.test(dep.name)
    )
      failures.push(`${pkg.name} imports a concrete SDK ${dep.name}`);
    if (
      role === 'application' &&
      localRole &&
      !['domain', 'application-contracts'].includes(localRole)
    )
      failures.push(`${pkg.name} depends on concrete or outer layer ${dep.name}`);
    if (
      role === 'desktop-protocol' &&
      localRole &&
      !['domain', 'application', 'application-contracts'].includes(localRole)
    )
      failures.push(`${pkg.name} depends on I/O layer ${dep.name}`);
    if (
      role === 'application-contracts' &&
      ((localRole && localRole !== 'domain') ||
        /^(tokio|rusqlite|russh|security-framework)/.test(dep.name))
    )
      failures.push(`${pkg.name} must remain an execution-free application contract: ${dep.name}`);
    if (
      role === 'local-files' &&
      localRole &&
      !['domain', 'application-contracts'].includes(localRole)
    )
      failures.push(`${pkg.name} must only implement application file ports: ${dep.name}`);
    if (role !== 'backend' && localRole === 'backend')
      failures.push(`${pkg.name} depends on Backend`);
  }
}
for (const name of ['core', 'host', 'runtime', 'connection']) {
  if (local.has(`${app}-${name}`))
    failures.push(`unsplit legacy responsibility crate: ${app}-${name}`);
}
if (existsSync(`crates/${app}-backend/src/connections`))
  failures.push('connection use cases must not live inside Backend');
/** @param {string} directory */
function inspectWire(directory) {
  if (!existsSync(directory)) return;
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === 'generated') continue;
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) inspectWire(path);
    else if (/\.tsx?$/.test(path) && /from ["']zod["']/.test(readFileSync(path, 'utf8')))
      failures.push(`${path}: wire boundary must use generated Schema, not Zod`);
  }
}
for (const layer of ['main', 'shared', 'contracts']) inspectWire(`src/${layer}`);
if (failures.length) {
  console.error(`Foundation boundaries (${root}):\n${failures.map((s) => `- ${s}`).join('\n')}`);
  process.exitCode = 1;
} else console.log(`Foundation boundaries: ${local.size} crates verified`);
