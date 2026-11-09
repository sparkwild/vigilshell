import { readFileSync } from 'node:fs';
// Build and signature layout checks consume the same Rust constants as negotiation/schema.
function version(path, name) {
  const source = readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
  const match = source.match(new RegExp(`pub const ${name}: u8 = (\\d+);`));
  if (!match) throw Error(`Missing protocol version: ${name}`);
  return Number(match[1]);
}
export const desktopProtocolVersion = version(
  'crates/vigilshell-desktop-protocol/src/v3.rs',
  'PROTOCOL_VERSION',
);
export const workspaceProtocolVersion = version(
  'crates/vigilshell-desktop-protocol/src/workspace.rs',
  'WORKSPACE_PROTOCOL_VERSION',
);
