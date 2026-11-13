import { afterEach, expect, it, vi } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const network = vi.hoisted(() => ({ check: vi.fn(), download: vi.fn() }));
vi.mock('electron', () => ({ autoUpdater: {} }));
vi.mock('electron-updater', async () => {
  const { EventEmitter } = await import('node:events');
  const { CancellationToken } = await import('builder-util-runtime');
  return {
    default: {
      CancellationToken,
      MacUpdater: class extends EventEmitter {
        setFeedURL() {}
        checkForUpdates() {
          return network.check();
        }
        downloadUpdate() {
          return network.download();
        }
      },
    },
  };
});
import { MacUpdateEngine } from './mac-engine';
let directory: string;
afterEach(async () => {
  if (directory) await rm(directory, { recursive: true, force: true });
});
it('cancels metadata discovery without starting a late archive download', async () => {
  directory = await mkdtemp(join(tmpdir(), 'vigilshell-engine-'));
  let respond!: (value: unknown) => void;
  network.check.mockReturnValue(
    new Promise((resolve) => {
      respond = resolve;
    }),
  );
  const engine = new MacUpdateEngine(
    directory,
    '0.1.1',
    'arm64',
    true,
    () => 'http://127.0.0.1/fixture/',
  );
  const download = engine.download({ version: '0.1.2', tag: 'v0.1.2', notes: '' }, vi.fn());
  const rejected = expect(download).rejects.toThrow('cancelled');
  await vi.waitFor(() => expect(network.check).toHaveBeenCalledOnce());
  expect(engine.cancel()).toBe(true);
  await rejected;
  respond({ isUpdateAvailable: true });
  await Promise.resolve();
  expect(network.download).not.toHaveBeenCalled();
});
