import type { Terminal } from '@xterm/xterm';

/** Keeps local session feedback outside xterm's selectable and searchable output. */
export class TerminalEndNotice {
  readonly element = document.createElement('div');
  private ended = false;

  constructor(
    private terminal: Terminal,
    private root: HTMLElement,
  ) {
    this.element.className = 'terminal-end-notice';
    this.element.setAttribute('role', 'status');
    this.element.hidden = true;
    root.append(this.element);
    terminal.onRender(() => this.update());
    terminal.onScroll(() => this.update());
    terminal.onWriteParsed(() => this.update());
  }

  state(ended: boolean, label: string) {
    this.ended = ended;
    this.element.textContent = label;
    this.update();
  }

  update() {
    const screen = this.terminal.element?.querySelector<HTMLElement>('.xterm-screen');
    if (!this.ended || !screen || !screen.clientHeight) {
      this.element.hidden = true;
      return;
    }
    const buffer = this.terminal.buffer.active;
    let lastLine = buffer.baseY + buffer.cursorY;
    // Full-screen programs can leave output below the final cursor position.
    for (let line = buffer.length - 1; line > lastLine; line--) {
      if (buffer.getLine(line)?.translateToString(true)) {
        lastLine = line;
        break;
      }
    }
    const row = lastLine + 1 - buffer.viewportY;
    this.element.hidden = row < 0 || row > this.terminal.rows;
    if (this.element.hidden) return;
    const screenRect = screen.getBoundingClientRect();
    const rootRect = this.root.getBoundingClientRect();
    this.element.style.top = `${screenRect.top - rootRect.top + (row * screen.clientHeight) / this.terminal.rows}px`;
    this.element.style.left = `${screenRect.left - rootRect.left}px`;
  }
}
