// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import type { Terminal } from '@xterm/xterm';
import { TerminalEndNotice } from './terminal-end-notice';

function fixture() {
  const root = document.createElement('div');
  const element = document.createElement('div');
  const screen = document.createElement('div');
  screen.className = 'xterm-screen';
  element.append(screen);
  root.append(element);
  Object.defineProperty(screen, 'clientHeight', { value: 240, configurable: true });
  const buffer = {
    baseY: 20,
    cursorY: 5,
    viewportY: 20,
    length: 32,
    getLine: vi.fn(() => ({ translateToString: (): string => '' })),
  };
  const terminal = {
    element,
    rows: 12,
    buffer: { active: buffer },
    onRender: vi.fn(),
    onScroll: vi.fn(),
    onWriteParsed: vi.fn(),
    write: vi.fn(),
  };
  const notice = new TerminalEndNotice(terminal as unknown as Terminal, root);
  return { root, screen, buffer, terminal, notice };
}

describe('local terminal end notice', () => {
  it('follows the output tail and disappears when scrolling into history without writing to the buffer', () => {
    const { notice, terminal, buffer } = fixture();
    notice.state(false, '连接已断开');
    expect(notice.element.hidden).toBe(true);
    notice.state(true, '连接已断开');
    expect(notice.element.hidden).toBe(false);
    expect(notice.element.style.top).toBe('120px');
    buffer.viewportY = 10;
    terminal.onScroll.mock.calls[0]![0]();
    expect(notice.element.hidden).toBe(true);
    buffer.viewportY = 20;
    buffer.cursorY = 11;
    terminal.onWriteParsed.mock.calls[0]![0]();
    expect(notice.element.hidden).toBe(false);
    expect(notice.element.style.top).toBe('240px');
    notice.state(true, 'Connection disconnected');
    expect(notice.element.textContent).toBe('Connection disconnected');
    expect(terminal.write).not.toHaveBeenCalled();
  });

  it('avoids output below the cursor and repositions after resize and late output', () => {
    const { notice, terminal, buffer, screen } = fixture();
    buffer.getLine.mockImplementation(() => ({ translateToString: () => 'last output' }));
    notice.state(true, 'Disconnected');
    expect(notice.element.style.top).toBe('240px');
    Object.defineProperty(screen, 'clientHeight', { value: 120 });
    terminal.onRender.mock.calls[0]![0]();
    expect(notice.element.style.top).toBe('120px');
    buffer.getLine.mockImplementation(() => ({ translateToString: (): string => '' }));
    terminal.onWriteParsed.mock.calls[0]![0]();
    expect(notice.element.style.top).toBe('60px');
  });
});
