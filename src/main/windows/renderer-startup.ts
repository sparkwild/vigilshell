export interface RendererFailure {
  code: string;
  detail: string;
}

interface StartupPorts {
  revealLoading(): void;
  reveal(): void;
  load(): Promise<void>;
  report(failure: RendererFailure): void;
  prompt(failure: RendererFailure, signal: AbortSignal): Promise<'retry' | 'dismiss'>;
}

/** 窗口恢复不能依赖失败的 Renderer；重试仅重载页面，不重启后端或重放操作。 */
export class RendererStartup {
  private phase: 'loading' | 'ready' | 'failed' | 'disposed' = 'loading';
  private painted = false;
  private wanted = true;
  private generation = 0;
  private failure: RendererFailure | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private promptController: AbortController | null = null;

  constructor(
    private readonly ports: StartupPorts,
    private readonly timeoutMs = 15_000,
  ) {}

  begin() {
    if (this.phase === 'disposed') return;
    this.clearPending();
    this.generation++;
    this.phase = 'loading';
    this.failure = null;
    this.timer = setTimeout(() => this.fail('renderer_startup_timeout'), this.timeoutMs);
  }

  windowReady() {
    this.painted = true;
    this.reveal();
  }

  rendererReady() {
    if (this.phase === 'disposed') return;
    this.clearPending();
    this.phase = 'ready';
    this.failure = null;
    this.reveal();
  }

  requestShow() {
    if (this.phase === 'disposed') return;
    this.wanted = true;
    if (this.phase === 'failed') void this.showFailure();
    else this.reveal();
  }

  hidden() {
    this.wanted = false;
    this.promptController?.abort();
    this.promptController = null;
  }

  fail(code: string, detail = '') {
    if (this.phase !== 'loading') return;
    clearTimeout(this.timer);
    this.phase = 'failed';
    this.failure = { code, detail: detail.replace(/\p{Cc}/gu, ' ').slice(0, 1000) };
    this.ports.report(this.failure);
    if (this.wanted) void this.showFailure();
  }

  crashed(reason: string) {
    this.begin();
    this.fail('renderer_process_gone', reason);
  }

  dispose() {
    this.clearPending();
    this.generation++;
    this.phase = 'disposed';
  }

  private reveal() {
    if (!this.painted || !this.wanted) return;
    if (this.phase === 'ready') this.ports.reveal();
    else if (this.phase === 'loading') this.ports.revealLoading();
  }

  private clearPending() {
    clearTimeout(this.timer);
    this.promptController?.abort();
    this.promptController = null;
  }

  private async showFailure() {
    if (this.promptController || !this.failure || this.phase !== 'failed') return;
    const generation = this.generation;
    const controller = new AbortController();
    this.promptController = controller;
    let decision: 'retry' | 'dismiss';
    try {
      decision = await this.ports.prompt(this.failure, controller.signal);
    } catch {
      if (!controller.signal.aborted && generation === this.generation)
        this.ports.report({ code: 'renderer_failure_prompt_failed', detail: '' });
      return;
    } finally {
      if (this.promptController === controller) this.promptController = null;
    }
    if (controller.signal.aborted || generation !== this.generation || this.phase !== 'failed')
      return;
    if (decision !== 'retry') return;
    this.begin();
    const retryGeneration = this.generation;
    try {
      await this.ports.load();
    } catch (error) {
      if (this.generation === retryGeneration)
        this.fail('renderer_load_failed', error instanceof Error ? error.message : '');
    }
  }
}
