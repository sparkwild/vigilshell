import { mkdir, mkdtemp, readdir, rename, rm } from 'node:fs/promises';
import { join } from 'node:path';

/** Publish already verified outputs, retaining unrelated channels and restoring
 * the previous outputs if any rename fails. Recovery remains outside the stage.
 * @param {string} stage
 * @param {string} output
 * @param {typeof rename} move
 */
export async function publishArtifacts(stage, output, move = rename) {
  const names = await readdir(stage);
  await mkdir(output, { recursive: true });
  const backup = await mkdtemp(join(output, '.publish-backup-'));
  /** @type {Array<{name:string, saved:boolean, installed:boolean}>} */
  const changes = [];
  try {
    for (const name of names) {
      const change = { name, saved: false, installed: false };
      changes.push(change);
      try {
        await move(join(output, name), join(backup, name));
        change.saved = true;
      } catch (error) {
        if (!(error instanceof Error) || !('code' in error) || error.code !== 'ENOENT') throw error;
      }
      await move(join(stage, name), join(output, name));
      change.installed = true;
    }
  } catch (error) {
    const failures = [error];
    for (const change of changes.reverse()) {
      try {
        if (change.installed) await move(join(output, change.name), join(stage, change.name));
        if (change.saved) await move(join(backup, change.name), join(output, change.name));
      } catch (rollbackError) {
        failures.push(rollbackError);
      }
    }
    if (failures.length > 1)
      throw new AggregateError(failures, `Restore artifacts from ${backup}`, { cause: error });
    await rm(backup, { recursive: true });
    throw error;
  }
  await rm(backup, { recursive: true });
}
