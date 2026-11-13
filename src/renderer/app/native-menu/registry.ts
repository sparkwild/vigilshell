import type { NativeMenuCommand, NativeMenuState, NativeMenuCloseTarget } from '@contracts';

export interface NativeMenuAction {
  enabled: boolean;
  run(): void | Promise<unknown>;
  checked?: boolean;
  closeTarget?: NativeMenuCloseTarget;
}
export type NativeMenuActions = Partial<Record<NativeMenuCommand, NativeMenuAction>>;

/** UI owners register their existing actions; one dispatcher picks the active owner. */
export class NativeMenuRegistry {
  private owners = new Map<string, { priority: number; actions: NativeMenuActions }>();
  changed = () => {};
  set(owner: string, priority: number, actions: NativeMenuActions) {
    this.owners.set(owner, { priority, actions });
    this.changed();
  }
  remove(owner: string) {
    this.owners.delete(owner);
    this.changed();
  }
  private actions() {
    const result: NativeMenuActions = {};
    for (const owner of [...this.owners.values()].sort((a, b) => a.priority - b.priority))
      Object.assign(result, owner.actions);
    return result;
  }
  snapshot(): NativeMenuState {
    const entries = Object.entries(this.actions()) as [NativeMenuCommand, NativeMenuAction][];
    return {
      enabled: entries
        .filter(([, action]) => action.enabled)
        .map(([key]) => key)
        .sort(),
      checked: entries
        .filter(([, action]) => action.checked)
        .map(([key]) => key)
        .sort(),
      closeTarget: this.actions().close?.closeTarget ?? 'window',
    };
  }
  dispatch(command: NativeMenuCommand) {
    const action = this.actions()[command];
    if (action?.enabled) return action.run();
  }
}
