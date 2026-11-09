import { spawn } from 'node:child_process';
const [seconds, command, ...args] = process.argv.slice(2);
if (!command || !/^[1-9]\d*$/.test(seconds)) throw Error('Expected timeout seconds and command');
const child = spawn(command, args, { stdio: 'inherit', detached: true });
let timedOut = false;
const timer = setTimeout(
  () => {
    timedOut = true;
    console.error('CI command timed out');
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch (error) {
      if (error.code !== 'ESRCH') console.error(error.message);
    }
    process.exitCode = 1;
  },
  Number(seconds) * 1000,
);
child.on('error', (error) => {
  clearTimeout(timer);
  console.error(error.message);
  process.exitCode = 1;
});
child.on('exit', (code) => {
  clearTimeout(timer);
  process.exitCode = timedOut ? 1 : (code ?? 1);
});
