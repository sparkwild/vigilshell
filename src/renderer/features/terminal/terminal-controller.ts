import { terminalFontStack } from './terminal-fonts';
import { TerminalEndNotice } from './terminal-end-notice';
import type { Preferences } from '@contracts';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import type { TerminalSessionView, TerminalSettings } from '@contracts';
import type { WorkspaceStore } from '../connections/workspace-store';
export class TerminalController {
  readonly terminal: Terminal;
  readonly search = new SearchAddon();
  private fitAddon = new FitAddon();
  private root = document.createElement('div');
  private display = document.createElement('div');
  private endNotice: TerminalEndNotice;
  private observer: ResizeObserver | null = null;
  private port: MessagePort | null = null;
  private outputSequence = 0;
  private inputSequence = 0;
  private inputQueue: Uint8Array[] = [];
  private queuedBytes = 0;
  private awaitingInput: number | null = null;
  private awaitingLength = 0;
  private portTimer: ReturnType<typeof setTimeout> | undefined;
  private inputTimer: ReturnType<typeof setTimeout> | undefined;
  private disposed = false;
  private opened = false;
  private fontTimer: ReturnType<typeof setTimeout> | undefined;
  private settings: TerminalSettings = {
    encoding: 'utf-8',
    backspace: 'del',
    delete: 'vt220',
  };
  private phase: TerminalSessionView['phase'] = 'connecting';
  private ended = false;
  private agentControlled = false;
  private cursorBlink = true;
  private promptLabel = '';
  private readonlyLabel = '';
  private endedLabel = '';
  private resizeInFlight = false;
  private desiredSize: { columns: number; rows: number } | null = null;
  private lastSize = '';
  private copyOnSelect = false;
  private selectionGesture = false;
  private selectionCopyFrame: number | undefined;
  private terminalTheme = 'inherit';
  private fontChoice = 'jetbrains-mono';
  private cancelSelectionCopy = () => {
    this.selectionGesture = false;
    if (this.selectionCopyFrame !== undefined) cancelAnimationFrame(this.selectionCopyFrame);
    this.selectionCopyFrame = undefined;
  };
  private onSelectionEnd = () => {
    if (!this.selectionGesture) return;
    this.selectionGesture = false;
    if (this.selectionCopyFrame !== undefined) cancelAnimationFrame(this.selectionCopyFrame);
    this.selectionCopyFrame = requestAnimationFrame(() => {
      this.selectionCopyFrame = undefined;
      if (
        this.disposed ||
        !this.copyOnSelect ||
        !this.root.isConnected ||
        this.root.closest('[hidden], [inert], [data-terminal-visible="false"]')
      )
        return;
      const text = this.terminal.getSelection();
      if (text)
        void window.vigilshell.copyText(text).catch(() => this.onError('clipboard_unavailable'));
    });
  };
  private pasteRunning = false;
  private capturePaste = false;
  private capturedPaste = '';
  private onError: (code: string) => void;
  constructor(
    readonly id: string,
    private workspace: WorkspaceStore,
    private paste: (text: string) => Promise<boolean>,
    onError: (code: string) => void,
  ) {
    this.onError = onError;
    this.portTimer = setTimeout(() => {
      if (!this.port && !this.disposed) this.fail('stream_unavailable');
    }, 15000);
    this.root.className = 'terminal-root';
    this.display.className = 'terminal-display';
    this.root.append(this.display);
    this.terminal = new Terminal({
      fontFamily: terminalFontStack('jetbrains-mono'),
      fontSize:
        Number.parseFloat(
          getComputedStyle(document.documentElement).getPropertyValue('--terminal-font-size'),
        ) || 12,
      lineHeight: 1.4,
      scrollback: 10000,
      cursorBlink: false,
      cursorInactiveStyle: 'none',
      disableStdin: true,
      allowProposedApi: false,
      logLevel: 'off',
    });
    this.terminal.loadAddon(this.fitAddon);
    this.terminal.loadAddon(this.search);
    this.endNotice = new TerminalEndNotice(this.terminal, this.root);
    this.root.addEventListener('focusin', () => {
      if (this.ended && document.activeElement === this.terminal.textarea)
        this.root.focus({ preventScroll: true });
    });
    this.appearance();
    this.terminal.parser.registerOscHandler(52, () => true);
    this.terminal.onData((data) => {
      if (this.capturePaste) this.capturedPaste += data;
      else this.enqueue(new TextEncoder().encode(data));
    });
    this.terminal.attachCustomKeyEventHandler((event) => {
      if (event.isComposing) return true;
      if (
        !event.metaKey &&
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        ['Backspace', 'Delete'].includes(event.key)
      ) {
        if (event.type === 'keydown' && this.phase === 'running') {
          const value =
            event.key === 'Backspace'
              ? this.settings.backspace === 'bs'
                ? '\x08'
                : '\x7f'
              : this.settings.delete === 'del'
                ? '\x7f'
                : '\x1b[3~';
          this.enqueue(new TextEncoder().encode(value));
        }
        event.preventDefault();
        return false;
      }
      if (event.metaKey && ['c', 'v', 'f', 'w', 'n'].includes(event.key.toLowerCase()))
        return false;
      return true;
    });
    this.root.addEventListener('pointerdown', (event) => {
      this.selectionGesture = this.copyOnSelect && event.button === 0;
    });
    document.addEventListener('pointerup', this.onSelectionEnd);
    this.root.addEventListener('pointercancel', this.cancelSelectionCopy);
    window.addEventListener('blur', this.cancelSelectionCopy);
    this.root.addEventListener('copy', (event) => {
      event.preventDefault();
      event.stopPropagation();
      event.clipboardData?.setData('text/plain', this.terminal.getSelection());
    });
    this.root.addEventListener(
      'paste',
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        void this.pasteText(event.clipboardData?.getData('text/plain') ?? '');
      },
      true,
    );
    this.fontTimer = setTimeout(() => {
      if (this.disposed) return;
      this.onError('font_unavailable');
      this.open();
    }, 3000);
    void Promise.all([
      document.fonts.load('12px "JetBrains Mono Variable"'),
      document.fonts.load('12px "Noto Sans SC Variable"', '中文'),
    ]).then(
      () => {
        if (this.disposed) return;
        clearTimeout(this.fontTimer);
        if (this.opened) {
          // A late font must invalidate xterm's cached metrics before fitting.
          const family = this.terminal.options.fontFamily!;
          this.terminal.options.fontFamily = `${family} `;
          this.terminal.options.fontFamily = family;
          this.fit();
        } else this.open();
      },
      () => {
        if (this.disposed) return;
        clearTimeout(this.fontTimer);
        this.onError('font_unavailable');
        this.open();
      },
    );
  }
  private open() {
    if (this.disposed || this.opened) return;
    this.appearance();
    this.terminal.open(this.display);
    this.opened = true;
    this.updateInputState();
    this.fit();
    if (this.phase === 'running' && this.root.isConnected) this.focus();
  }
  attach(element: HTMLElement) {
    element.append(this.root);
    this.appearance();
    this.observer?.disconnect();
    this.observer = new ResizeObserver(() => this.fit());
    this.observer.observe(element);
    this.fit();
  }
  detach() {
    this.cancelSelectionCopy();
    this.observer?.disconnect();
    this.observer = null;
    this.root.remove();
  }
  bind(port: MessagePort) {
    clearTimeout(this.portTimer);
    if (this.disposed) {
      port.close();
      return;
    }
    this.port?.close();
    this.port = port;
    port.onmessage = ({ data }: MessageEvent<unknown>) => {
      if (this.disposed || !data || typeof data !== 'object') return;
      const v = data as {
        kind?: string;
        sequence?: number;
        bytes?: Uint8Array;
        length?: number;
        code?: string;
      };
      if (
        v.kind === 'output' &&
        v.bytes instanceof Uint8Array &&
        v.sequence === this.outputSequence + 1
      ) {
        this.outputSequence = v.sequence;
        const length = v.bytes.byteLength,
          sequence = v.sequence;
        this.terminal.write(v.bytes, () => {
          if (!this.disposed && this.port === port)
            port.postMessage({ kind: 'credit', sequence, length });
        });
      } else if (
        v.kind === 'inputAck' &&
        v.sequence === this.awaitingInput &&
        v.length === this.awaitingLength
      ) {
        clearTimeout(this.inputTimer);
        this.awaitingInput = null;
        this.pump();
      } else if (v.kind === 'inputAck' && this.phase !== 'running') {
        return;
      } else if (v.kind === 'end') {
        if (!this.ended) {
          this.ended = true;
          this.phase = 'closed';
          this.updateInputState();
          this.workspace.terminalStreamEnded(this.id);
        }
        clearTimeout(this.inputTimer);
        this.inputQueue = [];
        this.queuedBytes = 0;
        this.awaitingInput = null;
      } else if (v.kind === 'error') {
        this.fail(v.code ?? 'stream_unavailable');
      } else this.fail('stream_unavailable');
    };
    port.onmessageerror = () => this.fail('stream_unavailable');
    port.start();
    this.pump();
  }
  state(session: TerminalSessionView) {
    this.agentControlled = !!session.agentControl;
    if (this.agentControlled) {
      this.inputQueue = [];
      this.queuedBytes = 0;
    }
    this.settings = session.terminal;
    if (this.ended && !['closed', 'failed', 'cancelled'].includes(session.phase)) return;
    const changed = this.phase !== session.phase;
    this.phase = session.phase;
    this.ended ||= ['closed', 'failed', 'cancelled'].includes(session.phase);
    this.updateInputState();
    if (changed && session.phase === 'running') {
      this.lastSize = '';
      this.fit();
      this.focus();
    }
    if (['closed', 'failed', 'cancelled'].includes(session.phase)) {
      clearTimeout(this.portTimer);
      clearTimeout(this.inputTimer);
      this.inputQueue = [];
      this.queuedBytes = 0;
      this.awaitingInput = null;
    }
  }
  private updateInputState() {
    const interactive = !this.ended && this.phase === 'running' && !this.agentControlled;
    const layoutChanged = this.root.dataset.ended !== String(this.ended);
    this.root.dataset.ended = String(this.ended);
    this.root.style.setProperty(
      '--terminal-end-height',
      `${Math.max(24, Math.ceil(this.terminal.options.fontSize! * this.terminal.options.lineHeight!) + 8)}px`,
    );
    this.terminal.options.disableStdin = !interactive;
    this.terminal.options.cursorBlink = interactive && this.cursorBlink;
    this.terminal.options.cursorInactiveStyle = interactive ? 'outline' : 'none';
    this.root.dataset.readonly = String(!interactive);
    this.root.tabIndex = interactive ? -1 : 0;
    if (this.terminal.textarea) {
      this.terminal.textarea.readOnly = !interactive;
      this.terminal.textarea.tabIndex = interactive ? 0 : -1;
      this.terminal.textarea.setAttribute('aria-readonly', String(!interactive));
      this.terminal.textarea.setAttribute(
        'aria-label',
        interactive ? this.promptLabel : this.readonlyLabel,
      );
      if (!interactive && document.activeElement === this.terminal.textarea)
        this.root.focus({ preventScroll: true });
    }
    this.appearance();
    if (layoutChanged) this.fit();
    this.endNotice.state(this.ended, this.endedLabel);
  }
  language(
    promptLabel: string,
    tooMuchOutput: string,
    readonlyLabel = promptLabel,
    endedLabel = readonlyLabel,
  ) {
    this.promptLabel = promptLabel;
    this.readonlyLabel = readonlyLabel;
    this.endedLabel = endedLabel;
    Terminal.strings.promptLabel = promptLabel;
    Terminal.strings.tooMuchOutput = tooMuchOutput;
    this.updateInputState();
  }
  preferences(p: Preferences) {
    this.copyOnSelect = p.terminalCopyOnSelect;
    this.terminalTheme = p.terminalThemeMode;
    if (this.fontChoice !== p.terminalFontFamily) {
      this.fontChoice = p.terminalFontFamily;
      this.terminal.options.fontFamily = terminalFontStack(this.fontChoice);
    }
    this.appearance();
    this.terminal.options.fontSize = p.terminalFontSize;
    this.terminal.options.lineHeight = p.terminalLineHeight / 10;
    this.terminal.options.scrollback = p.terminalScrollback;
    this.terminal.options.cursorStyle = p.terminalCursorStyle as 'block' | 'bar' | 'underline';
    this.cursorBlink = p.terminalCursorBlink;
    this.updateInputState();
    this.fit();
  }
  appearance() {
    const style = getComputedStyle(document.documentElement);
    const prefix =
      this.terminalTheme === 'inherit' ? '--terminal' : `--terminal-${this.terminalTheme}`;
    const color = (part: string) => style.getPropertyValue(`${prefix}-${part}`).trim();
    const background = color('background'),
      foreground = color('foreground'),
      selection = color('selection');
    this.root.style.setProperty('--terminal-background', background);
    this.root.style.setProperty('--terminal-foreground', foreground);
    this.root.parentElement?.style.setProperty('--terminal-background', background);
    this.terminal.options.theme = {
      background,
      foreground,
      cursor:
        this.phase === 'running' && !this.ended && !this.agentControlled
          ? foreground
          : 'transparent',
      selectionBackground: selection,
    };
  }
  focus() {
    if (
      this.disposed ||
      !this.root.isConnected ||
      this.root.closest('[hidden], [inert], [data-terminal-visible="false"]') ||
      document.querySelector(
        '[role="menu"][data-open], [aria-modal="true"][data-open], [data-terminal-session-list][data-open]',
      )
    )
      return;
    if (this.phase === 'running' && !this.ended && !this.agentControlled) this.terminal.focus();
    else this.root.focus({ preventScroll: true });
  }
  fit() {
    if (
      this.disposed ||
      !this.opened ||
      document.querySelector('[data-panel-motion="true"]') ||
      this.root.clientWidth < 2 ||
      this.root.clientHeight < 2
    )
      return;
    try {
      this.fitAddon.fit();
      this.endNotice.update();
    } catch {
      return;
    }
    const { cols, rows } = this.terminal;
    if (!cols || !rows) return;
    this.desiredSize = { columns: cols, rows };
    void this.resize();
  }
  private async resize() {
    if (this.resizeInFlight || this.phase !== 'running' || !this.desiredSize) return;
    const size = this.desiredSize,
      key = `${size.columns}:${size.rows}`;
    if (key === this.lastSize) return;
    this.resizeInFlight = true;
    try {
      await this.workspace.call('resizeTerminal', {
        sessionId: this.id,
        ...size,
      });
      this.lastSize = key;
    } catch {
      if (this.phase === 'running') this.onError('stream_unavailable');
    } finally {
      this.resizeInFlight = false;
      if (this.desiredSize && `${this.desiredSize.columns}:${this.desiredSize.rows}` !== key)
        void this.resize();
    }
  }
  private enqueue(bytes: Uint8Array) {
    if (this.agentControlled || this.phase !== 'running' || !bytes.length) return;
    if (this.queuedBytes + bytes.length > 256 * 1024) {
      this.onError('input_backpressure');
      return;
    }
    for (let i = 0; i < bytes.length; i += 32768) this.inputQueue.push(bytes.slice(i, i + 32768));
    this.queuedBytes += bytes.length;
    this.pump();
  }
  private pump() {
    if (
      this.agentControlled ||
      !this.port ||
      this.awaitingInput !== null ||
      this.phase !== 'running'
    )
      return;
    const bytes = this.inputQueue.shift();
    if (!bytes) return;
    this.queuedBytes -= bytes.length;
    this.awaitingInput = ++this.inputSequence;
    this.awaitingLength = bytes.length;
    this.inputTimer = setTimeout(() => this.fail('stream_unavailable'), 15000);
    this.port.postMessage({
      kind: 'input',
      sequence: this.inputSequence,
      bytes,
    });
  }
  private fail(code: string) {
    if (this.ended || this.disposed) return;
    clearTimeout(this.inputTimer);
    void this.workspace.call('closeTerminal', { sessionId: this.id }).catch(() => undefined);
    this.phase = 'failed';
    this.ended = true;
    this.updateInputState();
    this.workspace.terminalStreamEnded(this.id, code);
    this.inputQueue = [];
    this.queuedBytes = 0;
    this.onError(code);
  }
  async pasteClipboard() {
    await this.pasteText(await window.vigilshell.readClipboard());
  }
  async pasteText(text: string) {
    if (this.agentControlled || this.pasteRunning || this.phase !== 'running' || !text) return;
    this.pasteRunning = true;
    try {
      if (/[\r\n]/.test(text) && !(await this.paste(text))) return;
      if (new TextEncoder().encode(text).length > 4 * 1024 * 1024) {
        this.onError('input_backpressure');
        return;
      }
      if (this.settings.encoding !== 'utf-8') {
        for (let offset = 0; offset < text.length;) {
          let end = Math.min(offset + 4096, text.length);
          if (end < text.length && /[\uD800-\uDBFF]/.test(text[end - 1]!)) end--;
          try {
            await this.workspace.call('validateTerminalInput', {
              sessionId: this.id,
              text: text.slice(offset, end),
            });
          } catch (error) {
            this.onError(
              error instanceof Error && 'code' in error ? String(error.code) : 'unmappable_input',
            );
            return;
          }
          if (this.agentControlled || this.disposed || this.phase !== 'running') return;
          offset = end;
        }
      }
      if (this.agentControlled || this.disposed || this.phase !== 'running') return;
      // Let xterm normalize the paste and supply one pair of negotiated bracket markers.
      this.capturePaste = true;
      this.capturedPaste = '';
      try {
        this.terminal.paste(text);
      } finally {
        this.capturePaste = false;
      }
      const input = this.capturedPaste;
      this.capturedPaste = '';
      for (let offset = 0; offset < input.length;) {
        while (this.queuedBytes > 128 * 1024 && this.phase === 'running' && !this.disposed)
          await new Promise<void>((r) => setTimeout(r, 20));
        if (this.agentControlled || this.phase !== 'running' || this.disposed) return;
        let end = Math.min(offset + 8192, input.length);
        if (
          end < input.length &&
          input.charCodeAt(end - 1) >= 0xd800 &&
          input.charCodeAt(end - 1) <= 0xdbff
        )
          end--;
        this.enqueue(new TextEncoder().encode(input.slice(offset, end)));
        offset = end;
      }
    } finally {
      this.pasteRunning = false;
    }
  }
  dispose() {
    this.disposed = true;
    clearTimeout(this.portTimer);
    clearTimeout(this.inputTimer);
    this.observer?.disconnect();
    clearTimeout(this.fontTimer);
    this.port?.close();
    document.removeEventListener('pointerup', this.onSelectionEnd);
    window.removeEventListener('blur', this.cancelSelectionCopy);
    if (this.selectionCopyFrame !== undefined) cancelAnimationFrame(this.selectionCopyFrame);
    this.terminal.dispose();
    this.root.remove();
    this.inputQueue = [];
  }
}
