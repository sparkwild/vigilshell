// @vitest-environment jsdom
import { defaultPreferences } from '../../../shared/preferences';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TerminalSessionView } from '@contracts';
import type { WorkspaceStore } from '../connections/workspace-store';
import { TerminalController } from './terminal-controller';

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    static strings = { promptLabel: '', tooMuchOutput: '' };
    options: Record<string, unknown>;
    cols = 80;
    rows = 24;
    parser = { registerOscHandler: vi.fn() };
    open = vi.fn((element: HTMLElement) => element.append(this.textarea));
    focus = vi.fn();
    dispose = vi.fn();
    loadAddon = vi.fn();
    onData = vi.fn();
    onRender = vi.fn();
    onScroll = vi.fn();
    onWriteParsed = vi.fn();
    textarea = document.createElement('textarea');
    paste = vi.fn();
    write = vi.fn((_bytes: Uint8Array, done?: () => void) => done?.());
    getSelection = vi.fn(() => 'selected output');
    attachCustomKeyEventHandler = vi.fn();
    constructor(options: Record<string, unknown>) {
      this.options = options;
    }
  },
}));
vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit = vi.fn();
  },
}));
vi.mock('@xterm/addon-search', () => ({ SearchAddon: class {} }));

describe('terminal font initialization', () => {
  let resolveFonts: () => void;
  let rejectFonts: () => void;
  let controller: TerminalController;
  let error: ReturnType<typeof vi.fn<(code: string) => void>>;
  const originalFonts = Object.getOwnPropertyDescriptor(document, 'fonts');

  beforeEach(() => {
    vi.useFakeTimers();
    const loaded = new Promise<void>((resolve, reject) => {
      resolveFonts = resolve;
      rejectFonts = () => reject(new Error('font load failed'));
    });
    Object.defineProperty(document, 'fonts', {
      configurable: true,
      value: { load: vi.fn(() => loaded) },
    });
    error = vi.fn();
    controller = new TerminalController(
      'session',
      { call: vi.fn(async () => ({})), terminalStreamEnded: vi.fn() } as unknown as WorkspaceStore,
      async () => true,
      error,
    );
  });
  afterEach(() => {
    controller.dispose();
    vi.useRealTimers();
    if (originalFonts) Object.defineProperty(document, 'fonts', originalFonts);
    else Reflect.deleteProperty(document, 'fonts');
  });
  async function flushFonts() {
    await Promise.resolve();
    await Promise.resolve();
  }

  it('waits for cold fonts before opening and measuring xterm', async () => {
    expect(controller.terminal.open).not.toHaveBeenCalled();
    resolveFonts();
    await flushFonts();
    expect(controller.terminal.open).toHaveBeenCalledTimes(1);
    expect(error).not.toHaveBeenCalled();
  });

  it('opens a fallback on timeout and invalidates metrics when fonts arrive', async () => {
    vi.advanceTimersByTime(3000);
    expect(controller.terminal.open).toHaveBeenCalledTimes(1);
    expect(error).toHaveBeenCalledWith('font_unavailable');
    const options = controller.terminal.options;
    const family = options.fontFamily;
    const changes: unknown[] = [];
    Object.defineProperty(options, 'fontFamily', {
      configurable: true,
      get: () => family,
      set: (value) => changes.push(value),
    });
    resolveFonts();
    await flushFonts();
    expect(changes).toEqual([`${family} `, family]);
    expect(controller.terminal.open).toHaveBeenCalledTimes(1);
  });

  it('falls back when fonts fail', async () => {
    rejectFonts();
    await flushFonts();
    expect(controller.terminal.open).toHaveBeenCalledTimes(1);
    expect(error).toHaveBeenCalledWith('font_unavailable');
  });

  it('does not open or report errors after disposal', async () => {
    controller.dispose();
    resolveFonts();
    await flushFonts();
    vi.advanceTimersByTime(3000);
    expect(controller.terminal.open).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
  });
  it('does not focus hidden terminals or steal menu/modal focus, and reuses the display when shown', async () => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        disconnect() {}
      },
    );
    const panel = document.createElement('section');
    panel.dataset.terminalVisible = 'false';
    document.body.append(panel);
    controller.attach(panel);
    resolveFonts();
    await flushFonts();
    controller.focus();
    expect(controller.terminal.focus).not.toHaveBeenCalled();
    panel.dataset.terminalVisible = 'true';
    const popup = document.createElement('div');
    popup.setAttribute('role', 'menu');
    popup.setAttribute('data-open', '');
    document.body.append(popup);
    controller.focus();
    expect(controller.terminal.focus).not.toHaveBeenCalled();
    popup.setAttribute('role', 'dialog');
    popup.setAttribute('aria-modal', 'true');
    controller.focus();
    expect(controller.terminal.focus).not.toHaveBeenCalled();
    popup.remove();
    controller.state({
      phase: 'running',
      terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
    } as TerminalSessionView);
    vi.mocked(controller.terminal.focus).mockClear();
    controller.focus();
    expect(controller.terminal.focus).toHaveBeenCalledTimes(1);
    panel.hidden = true;
    controller.focus();
    expect(controller.terminal.focus).toHaveBeenCalledTimes(1);
    expect(controller.terminal.open).toHaveBeenCalledTimes(1);
    panel.remove();
    vi.unstubAllGlobals();
  });

  it('changes display preferences without replacing the terminal and keeps the terminal font independent', () => {
    const original = controller.terminal;
    controller.preferences({
      ...defaultPreferences,
      terminalFontSize: 18,
      terminalLineHeight: 16,
      terminalCursorStyle: 'bar',
      terminalCursorBlink: false,
      terminalScrollback: 5000,
    });
    expect(controller.terminal).toBe(original);
    expect(controller.id).toBe('session');
    expect(original.options).toMatchObject({
      fontSize: 18,
      lineHeight: 1.6,
      scrollback: 5000,
      cursorStyle: 'bar',
      cursorBlink: false,
    });
  });
  it('uses the terminal theme independently of the app and follows the app again on request', () => {
    const root = document.documentElement;
    root.style.setProperty('--terminal-background', '#ffffff');
    root.style.setProperty('--terminal-foreground', '#262626');
    root.style.setProperty('--terminal-selection', '#d4d4d4');
    root.style.setProperty('--terminal-dark-background', '#262626');
    root.style.setProperty('--terminal-dark-foreground', '#e5e5e5');
    root.style.setProperty('--terminal-dark-selection', '#525252');
    controller.preferences({ ...defaultPreferences, terminalThemeMode: 'dark' });
    expect(controller.terminal.options.theme).toMatchObject({
      background: '#262626',
      foreground: '#e5e5e5',
    });
    expect(root.style.getPropertyValue('--terminal-background')).toBe('#ffffff');
    controller.preferences(defaultPreferences);
    expect(controller.terminal.options.theme).toMatchObject({
      background: '#ffffff',
    });
    for (const key of ['background', 'foreground', 'selection']) {
      root.style.removeProperty(`--terminal-${key}`);
      root.style.removeProperty(`--terminal-dark-${key}`);
    }
  });
  it('copies only completed mouse selections when enabled and cancels pending copies on detach', async () => {
    const copyText = vi.fn(async () => {});
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        disconnect() {}
      },
    );
    Object.assign(window, { vigilshell: { copyText } });
    const surface = document.createElement('div');
    document.body.append(surface);
    controller.attach(surface);
    const root = surface.firstElementChild!;
    const select = () => {
      root.dispatchEvent(new MouseEvent('pointerdown', { button: 0, bubbles: true }));
      document.dispatchEvent(new MouseEvent('pointerup', { button: 0, bubbles: true }));
    };
    select();
    await vi.advanceTimersByTimeAsync(20);
    expect(copyText).not.toHaveBeenCalled();
    controller.preferences({
      ...defaultPreferences,
      terminalCopyOnSelect: true,
    });
    select();
    await vi.advanceTimersByTimeAsync(20);
    expect(copyText).toHaveBeenCalledWith('selected output');
    const keyHandler = vi.mocked(controller.terminal.attachCustomKeyEventHandler).mock.calls[0]![0];
    expect(keyHandler(new KeyboardEvent('keydown', { key: 'c', ctrlKey: true }))).toBe(true);
    select();
    controller.detach();
    await vi.advanceTimersByTimeAsync(20);
    expect(copyText).toHaveBeenCalledTimes(1);
    surface.remove();
    vi.unstubAllGlobals();
  });
  it('locks input and paste while an agent owns the tab and restores the same terminal on release', async () => {
    const running = {
      phase: 'running',
      terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
    } as TerminalSessionView;
    const port = {
      onmessage: null,
      onmessageerror: null,
      start: vi.fn(),
      close: vi.fn(),
      postMessage: vi.fn(),
    } as unknown as MessagePort;
    controller.bind(port);
    controller.state(running);
    const terminal = controller.terminal;
    const send = vi.mocked(terminal.onData).mock.calls[0]![0];
    controller.state({
      ...running,
      agentControl: {
        entryId: 'agent',
        entryName: 'DeepSeek Harness',
        contextId: 'context',
        leaseId: 'one',
        releasing: false,
      },
    });
    send('accidental');
    await controller.pasteText('clipboard');
    expect(port.postMessage).not.toHaveBeenCalled();
    expect(terminal.paste).not.toHaveBeenCalled();
    expect(terminal.options.disableStdin).toBe(true);
    expect(terminal.getSelection()).toBe('selected output');
    controller.state(running);
    send('manual');
    expect(port.postMessage).toHaveBeenCalledOnce();
    expect(terminal.options.disableStdin).toBe(false);
    controller.state({
      ...running,
      agentControl: {
        entryId: 'agent',
        entryName: 'DeepSeek Harness',
        contextId: 'next-context',
        leaseId: 'two',
        releasing: false,
      },
    });
    send('blocked again');
    expect(port.postMessage).toHaveBeenCalledOnce();
    expect(controller.terminal).toBe(terminal);
    expect(port.close).not.toHaveBeenCalled();
  });
  it('makes ended streams read-only, clears queued input, and rejects late running snapshots and preferences', async () => {
    const running = {
      phase: 'running',
      terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
    } as TerminalSessionView;
    const port = {
      onmessage: null,
      onmessageerror: null,
      start: vi.fn(),
      close: vi.fn(),
      postMessage: vi.fn(),
    } as unknown as MessagePort;
    controller.bind(port);
    controller.state(running);
    controller.language('Input', 'Output limit', 'Read-only history');
    expect(controller.terminal.options.disableStdin).toBe(false);
    const send = vi.mocked(controller.terminal.onData).mock.calls[0]![0];
    send('first');
    send('queued');
    expect(port.postMessage).toHaveBeenCalledTimes(1);
    port.onmessage!({ data: { kind: 'end' } } as MessageEvent);
    controller.state(running);
    controller.preferences({ ...defaultPreferences, terminalCursorBlink: true });
    controller.appearance();
    send('late');
    await controller.pasteText('paste');
    expect(port.postMessage).toHaveBeenCalledTimes(1);
    expect(controller.terminal.paste).not.toHaveBeenCalled();
    expect(controller.terminal.options).toMatchObject({
      disableStdin: true,
      cursorBlink: false,
      cursorInactiveStyle: 'none',
      theme: { cursor: 'transparent' },
    });
    expect(controller.terminal.textarea).toHaveProperty('readOnly', true);
    expect(controller.terminal.textarea?.getAttribute('aria-label')).toBe('Read-only history');
    expect(controller.terminal.getSelection()).toBe('selected output');
  });
  it('redirects xterm focus after disconnection so clicking output cannot restore the cursor', async () => {
    resolveFonts();
    await flushFonts();
    const display = vi.mocked(controller.terminal.open).mock.calls[0]![0];
    const root = display.parentElement!;
    document.body.append(root);
    controller.state({
      phase: 'closed',
      terminal: { encoding: 'utf-8', backspace: 'del', delete: 'vt220' },
    } as TerminalSessionView);
    controller.terminal.textarea!.focus();
    expect(document.activeElement).toBe(root);
    expect(controller.terminal.options.cursorInactiveStyle).toBe('none');
    expect(controller.terminal.getSelection()).toBe('selected output');
  });
});
