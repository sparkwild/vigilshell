import { execFileSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

/** Stop the launcher's detached group once, including when exit and signal race.
 * Some macOS groups briefly contain only exiting children after their leader dies.
 * Accept EPERM only after observing no live group members; never hide surviving
 * processes behind a successful cleanup result.
 * @param {number|undefined} pid
 * @param {{kill:(pid:number, signal:NodeJS.Signals)=>unknown, hasMembers:(group:number)=>boolean}} system
 */
export function createProcessGroupStopper(
  pid,
  system = {
    kill: (group, signal) => process.kill(group, signal),
    hasMembers: (group) =>
      execFileSync('/bin/ps', ['-axo', 'pid=,pgid=,stat='], { encoding: 'utf8' })
        .split('\n')
        .some((line) => {
          const [, id, state] = line.trim().split(/\s+/);
          return Number(id) === group && !state?.startsWith('Z');
        }),
  },
) {
  /** @type {Promise<void>|undefined} */
  let stopping;
  return () =>
    (stopping ??= (async () => {
      if (!pid) return;
      try {
        system.kill(-pid, 'SIGTERM');
      } catch (error) {
        const code = error && typeof error === 'object' && 'code' in error ? error.code : null;
        if (code === 'ESRCH') return;
        if (code === 'EPERM') {
          for (let attempt = 0; attempt < 25; attempt++) {
            if (!system.hasMembers(pid)) return;
            await delay(40);
          }
          if (!system.hasMembers(pid)) return;
        }
        throw error;
      }
    })());
}
