import type { TerminalSessionView } from '@contracts';

export const terminalTabId = (id: string) => `terminal-tab-${id}`;
export const terminalPanelId = (id: string) => `terminal-panel-${id}`;
export function terminalLabel(
  session: TerminalSessionView,
  name: string | undefined,
  ordinal: number,
) {
  const label = name?.trim() || session.host;
  return ordinal > 1 ? `${label} · ${ordinal}` : label;
}

/** 菜单完成焦点恢复时才交回输入；隐藏的终端不能接收按键。 */
export function terminalFocusTarget(id: string): HTMLElement | null {
  const panel = document.getElementById(terminalPanelId(id));
  if (!panel || panel.dataset.terminalVisible !== 'true' || panel.closest('[hidden], [inert]'))
    return null;
  return panel.querySelector<HTMLElement>('.xterm-helper-textarea');
}
