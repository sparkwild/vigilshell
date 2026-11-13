import { Menu, ipcMain, type Tray, type IpcMainEvent } from 'electron';
import type { NativeMenuState, ResolvedLocale } from '@contracts';
import { contractValidator } from '../../contracts/validation';
import { translate } from '../../shared/i18n/messages';
import { applicationMenuLayout, trayMenuLayout, emptyMenuState, type MenuActions } from './layout';

export class NativeMenus {
  private state: NativeMenuState = emptyMenuState;
  private locale: ResolvedLocale = 'en';
  private blocked = false;
  private rendered = false;
  private tray?: Tray;
  constructor(
    private name: string,
    private actions: MenuActions,
    allowed: (event: IpcMainEvent) => boolean,
  ) {
    const validate = contractValidator<NativeMenuState>('NativeMenuState');
    ipcMain.on('workspace:menu-state', (event, value: unknown) => {
      if (
        !allowed(event) ||
        !validate(value) ||
        JSON.stringify(value) === JSON.stringify(this.state)
      )
        return;
      this.state = value;
      this.render();
    });
  }
  update(locale: ResolvedLocale, blocked = this.blocked) {
    if (this.rendered && this.locale === locale && this.blocked === blocked) return;
    this.locale = locale;
    this.blocked = blocked;
    this.render();
  }
  setTray(tray: Tray | undefined) {
    this.tray = tray;
    this.render();
  }
  reset() {
    this.state = emptyMenuState;
    this.render();
  }
  private render() {
    this.rendered = true;
    const actions: MenuActions = {
      ...this.actions,
      command: (command) => {
        if (!this.blocked && this.state.enabled.includes(command)) this.actions.command(command);
      },
    };
    const t = (key: Parameters<typeof translate>[0]) => translate(key, this.locale);
    Menu.setApplicationMenu(
      Menu.buildFromTemplate(
        applicationMenuLayout(this.name, process.platform, t, actions, this.state, this.blocked),
      ),
    );
    this.tray?.setContextMenu(Menu.buildFromTemplate(trayMenuLayout(t, actions, this.blocked)));
    this.tray?.setToolTip(this.name);
  }
}
