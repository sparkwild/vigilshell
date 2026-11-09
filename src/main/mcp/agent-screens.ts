import { gzip, gunzip } from 'node:zlib';
import { promisify } from 'node:util';
import headless from '@xterm/headless';
import serialize from '@xterm/addon-serialize';
import type { Terminal as BrowserTerminal } from '@xterm/xterm';
import type { Terminal as HeadlessTerminal } from '@xterm/headless';
import type { ActivityRecord, AgentEvent, AgentOperation, AgentReply, AgentTask } from '@contracts';
const compress = promisify(gzip),
  expand = promisify(gunzip);
const { Terminal } = headless;
const { SerializeAddon } = serialize;
type Screen = {
  task: AgentTask;
  terminal: HeadlessTerminal;
  serialize: InstanceType<typeof SerializeAddon>;
  pending: Promise<void>;
  queuedBytes: number;
  cursor: number;
  seen: number;
  recovering: boolean;
  behind: boolean;
  incomplete: boolean;
  touched: number;
  limit: number;
  checkpointAt: number;
  checkpointBytes: number;
  checkpointPending: boolean;
};
/** Main reconstructs MCP terminal screens from records; UI visibility never controls reads. */
export class AgentScreens {
  private readonly states = new Map<string, Screen>();
  private stopped = false;
  constructor(
    private readonly request: (operation: AgentOperation) => Promise<AgentReply>,
    private readonly scrollback: () => number,
  ) {}
  private create(task: AgentTask) {
    const operation = task.operation;
    const columns = operation?.kind === 'terminal' ? operation.columns : 120,
      rows = operation?.kind === 'terminal' ? operation.rows : 40;
    // 队列的内存预算只控制显示追赶，不能截断持久输出或暂停远端执行。
    const limit = 4 * 1024 * 1024;
    const terminal = new Terminal({
      cols: columns,
      rows,
      convertEol: !task.terminal,
      scrollback: this.scrollbackRows(),
      allowProposedApi: true,
    });
    const addon = new SerializeAddon();
    // The serializer uses xterm's common buffer API, shared by headless and browser terminals.
    terminal.loadAddon({
      activate: (value) => addon.activate(value as unknown as BrowserTerminal),
      dispose: () => addon.dispose(),
    });
    terminal.parser.registerOscHandler(52, () => true);
    terminal.parser.registerOscHandler(8, () => true);
    const state: Screen = {
      checkpointAt: 0,
      checkpointBytes: 0,
      checkpointPending: false,
      task,
      terminal,
      serialize: addon,
      pending: Promise.resolve(),
      queuedBytes: 0,
      cursor: 0,
      seen: 0,
      recovering: false,
      behind: false,
      incomplete: false,
      touched: Date.now(),
      limit,
    };
    this.states.set(task.id, state);
    if (this.states.size > 80) {
      const disposable = [...this.states.values()]
        .filter((value) => !['queued', 'running', 'stopping'].includes(value.task.phase))
        .sort((a, b) => a.touched - b.touched)[0];
      if (disposable && disposable !== state) {
        this.states.delete(disposable.task.id);
        void disposable.pending.finally(() => disposable.terminal.dispose());
      }
    }
    return state;
  }
  private scrollbackRows() {
    const rows = this.scrollback();
    return [1000, 5000, 10000, 20000, 50000].includes(rows) ? rows : 10000;
  }
  refreshPreferences() {
    if (this.stopped) return;
    const rows = this.scrollbackRows();
    for (const state of this.states.values()) {
      if (state.terminal.options.scrollback !== rows) state.terminal.options.scrollback = rows;
    }
  }
  event(event: AgentEvent) {
    if (this.stopped) return;
    if (event.kind === 'task') {
      const state = this.states.get(event.task.id);
      if (state) state.task = event.task;
      else if (event.task.operation) this.create(event.task);
    }
    if (event.kind !== 'activity' || !event.record.taskId || event.record.body.kind === 'screen')
      return;
    const state = this.states.get(event.record.taskId);
    if (!state || event.record.sequence <= state.cursor) return;
    state.seen = Math.max(state.seen, event.record.sequence);
    const bytes = event.record.body.kind === 'output' ? event.record.body.text.length * 2 : 0;
    if (state.recovering || state.behind || state.queuedBytes + bytes > state.limit / 4) {
      state.behind = true;
      if (!state.recovering) this.recover(state);
      return;
    }
    state.queuedBytes += bytes;
    state.pending = state.pending
      .then(async () => {
        try {
          await this.apply(state, event.record);
          if (event.record.body.kind === 'output') void this.checkpoint(state);
        } finally {
          state.queuedBytes -= bytes;
        }
      })
      .catch(() => {
        state.incomplete = true;
      });
  }
  private async apply(state: Screen, record: ActivityRecord) {
    if (record.sequence <= state.cursor || this.stopped) return;
    const body = record.body;
    if (body.kind === 'output' && body.text) {
      state.checkpointBytes += Buffer.byteLength(body.text);
      await new Promise<void>((resolve) => state.terminal.write(body.text, resolve));
    }
    if (body.kind === 'resize') state.terminal.resize(body.columns, body.rows);
    if (body.kind === 'gap') state.incomplete = true;
    state.cursor = record.sequence;
  }
  private recover(state: Screen) {
    if (state.recovering || this.stopped) return;
    state.recovering = true;
    const target = state.seen;
    state.pending = state.pending
      .then(async () => {
        do {
          const reply = await this.request({
            operation: 'activity',
            session_id: state.task.contextId,
            task_id: state.task.id,
            cursor: state.cursor,
          });
          if (reply.kind !== 'activity') throw Error('activity_recording_unavailable');
          state.incomplete ||= reply.page.incomplete;
          for (const record of reply.page.records) await this.apply(state, record);
          if (!reply.page.records.length || state.cursor >= target || this.stopped) break;
        } while (!this.stopped);
        state.behind = state.cursor < state.seen;
      })
      .catch(() => {
        state.incomplete = true;
        state.behind = false;
      })
      .finally(() => {
        state.recovering = false;
        if (state.behind && !this.stopped) this.recover(state);
      });
  }
  private async checkpoint(state: Screen) {
    if (
      state.checkpointPending ||
      state.recovering ||
      !state.cursor ||
      Date.now() - state.checkpointAt < 10000 ||
      (state.checkpointAt !== 0 && state.checkpointBytes < 8 * 1024 * 1024) ||
      !['queued', 'running', 'stopping'].includes(state.task.phase)
    )
      return;
    state.checkpointPending = true;
    state.checkpointAt = Date.now();
    state.checkpointBytes = 0;
    const sequence = state.cursor,
      columns = state.terminal.cols,
      rows = state.terminal.rows;
    try {
      const bytes = Buffer.from(state.serialize.serialize());
      if (bytes.length > 32 * 1024 * 1024) return;
      const gzip = (await compress(bytes)).toString('base64');
      if (gzip.length > 1024 * 1024 || this.stopped) return;
      await this.request({
        operation: 'saveScreen',
        task_id: state.task.id,
        checkpoint: { version: 1, sequence, columns, rows, gzip },
      });
    } catch {
      /* The checkpoint is a derived cache; complete records remain the recovery source. */
    } finally {
      state.checkpointPending = false;
    }
  }
  private async restore(state: Screen) {
    try {
      const reply = await this.request({ operation: 'loadScreen', task_id: state.task.id });
      if (reply.kind !== 'screen' || !reply.checkpoint) return;
      const checkpoint = reply.checkpoint;
      if (
        checkpoint.version !== 1 ||
        checkpoint.sequence < 1 ||
        checkpoint.gzip.length > 1024 * 1024 ||
        checkpoint.columns < 1 ||
        checkpoint.columns > 500 ||
        checkpoint.rows < 1 ||
        checkpoint.rows > 300
      )
        throw Error('invalid_screen_checkpoint');
      const bytes = await expand(Buffer.from(checkpoint.gzip, 'base64'), {
        maxOutputLength: 32 * 1024 * 1024,
      });
      if (this.stopped) return;
      state.terminal.resize(checkpoint.columns, checkpoint.rows);
      await new Promise<void>((resolve) => state.terminal.write(bytes.toString('utf8'), resolve));
      state.cursor = checkpoint.sequence;
    } catch {
      state.terminal.reset();
      state.cursor = 0;
    }
  }
  async snapshot(id: string) {
    let state = this.states.get(id);
    if (!state) {
      const reply = await this.request({ operation: 'task', task_id: id });
      if (reply.kind !== 'task') throw Error('not_found');
      state = this.create(reply.task);
      state.pending = this.restore(state);
      await state.pending;
      state.seen = Math.max(reply.task.outputEnd, state.cursor);
      state.behind = true;
      this.recover(state);
    }
    if (state.behind && !state.recovering) this.recover(state);
    await state.pending;
    state.touched = Date.now();
    return {
      taskId: id,
      sequence: state.cursor,
      columns: state.terminal.cols,
      rows: state.terminal.rows,
      ansi: state.serialize.serialize(),
      incomplete: state.incomplete,
    };
  }
  async text(id: string) {
    await this.snapshot(id);
    const state = this.states.get(id)!;
    const buffer = state.terminal.buffer.active;
    return Array.from(
      { length: buffer.length },
      (_, index) => buffer.getLine(index)?.translateToString(true) ?? '',
    ).join('\n');
  }
  async close() {
    this.stopped = true;
    await Promise.allSettled([...this.states.values()].map((state) => state.pending));
    for (const state of this.states.values()) state.terminal.dispose();
    this.states.clear();
  }
}
