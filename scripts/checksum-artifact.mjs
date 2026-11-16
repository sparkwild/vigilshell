import { writeFileSync } from 'node:fs';
import { basename } from 'node:path';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

export default async function checksumArtifact(event) {
  if (!event.file?.endsWith('.dmg')) return;
  const digest = createHash('sha256').update(readFileSync(event.file)).digest('hex');
  writeFileSync(`${event.file}.sha256`, `${digest}  ${basename(event.file)}\n`);
}
