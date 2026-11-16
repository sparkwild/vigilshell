import electronUpdater from 'electron-updater';
import type { DownloadUpdateOptions } from 'electron-updater/out/AppUpdater';
import { verifiedInfo } from './cache';

/** Preserve the installed architecture, including x64 applications running under Rosetta. */
export class ArchitectureMacUpdater extends electronUpdater.MacUpdater {
  constructor(private arch: string) {
    super();
  }

  protected override doDownloadUpdate(options: DownloadUpdateOptions) {
    const { info, provider } = options.updateInfoAndProvider;
    const selected = verifiedInfo(
      info,
      { version: info.version, tag: `v${info.version}`, notes: '' },
      this.arch,
    );
    return super.doDownloadUpdate({
      ...options,
      updateInfoAndProvider: { info: selected, provider },
    });
  }
}
