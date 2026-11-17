import { afterEach, expect, it, vi } from 'vitest';
import type { UpdateInfo } from 'electron-updater';
import type { DownloadUpdateOptions } from 'electron-updater/out/AppUpdater';

const transfer = vi.hoisted(() => ({ start: vi.fn() }));
vi.mock('electron-updater', () => ({
  default: {
    MacUpdater: class {
      doDownloadUpdate(options: DownloadUpdateOptions) {
        return transfer.start(options.updateInfoAndProvider.info);
      }
    },
  },
}));
import { ArchitectureMacUpdater } from './architecture-updater';

class Probe extends ArchitectureMacUpdater {
  download(info: DownloadUpdateOptions['updateInfoAndProvider']['info']) {
    return this.doDownloadUpdate({ updateInfoAndProvider: { info } } as DownloadUpdateOptions);
  }
}
afterEach(() => vi.clearAllMocks());

it.each(['arm64', 'x64'])(
  'passes only the installed %s ZIP to the native updater',
  async (arch) => {
    const info = {
      version: '0.1.5',
      path: 'VigilShell-0.1.5-macos-arm64.zip',
      sha512: Buffer.alloc(64).toString('base64'),
      releaseDate: '2026-11-16T00:00:00.000Z',
      files: ['arm64', 'x64'].map((value) => ({
        url: `VigilShell-0.1.5-macos-${value}.zip`,
        size: 100,
        sha512: Buffer.alloc(64).toString('base64'),
      })),
    };
    await new Probe(arch).download(info);
    const selected = transfer.start.mock.calls[0]![0] as UpdateInfo;
    expect(selected.files).toEqual(info.files.filter((file) => file.url.includes(arch)));
    expect(info.files).toHaveLength(2);

    // Exercise the pinned upstream Rosetta/hardware filter against the actual narrowed metadata.
    const real = await vi.importActual<typeof import('electron-updater')>('electron-updater');
    class Filter extends real.MacUpdater {
      static select(isArm64Mac: boolean) {
        return this.filterFilesForArch(
          selected.files.map((file) => ({
            url: new URL(file.url, 'https://example.invalid/'),
            info: file,
          })),
          isArm64Mac,
        );
      }
    }
    expect(Filter.select(arch === 'arm64')).toHaveLength(1);
    if (arch === 'x64') expect(Filter.select(true)[0]?.url.pathname).toContain('x64.zip');
  },
);

it('refuses a missing architecture before starting a download', () => {
  expect(() =>
    new Probe('x64').download({
      version: '0.1.5',
      files: [],
      path: '',
      sha512: '',
      releaseDate: '',
    }),
  ).toThrow('invalid_update');
  expect(transfer.start).not.toHaveBeenCalled();
});
