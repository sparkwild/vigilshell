// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, act, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { AgentActivity } from './AgentActivity';
import { translate } from '../../../shared/i18n/messages';
import type { AgentEvent, AgentUiRequest, RecordedSession, AgentTask } from '@contracts';
const f = vi.hoisted(() => ({
  runtime: {
    status: 'ready' as 'ready' | 'starting' | 'unavailable',
    instanceId: 'runtime' as string | null,
  },
  request: vi.fn(),
  listeners: new Set<(event: AgentEvent) => void>(),
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    snapshot: { runtime: f.runtime },
    t: (key: Parameters<typeof translate>[0], params?: Record<string, string | number>) =>
      translate(key, 'zh-CN', params),
  }),
}));
vi.mock('./model', () => ({
  useMcp: () => ({ snapshot: { contexts: [{ id: 'live', phase: 'ready' }] }, error: null }),
}));
const recording = (id: string, ended = false): RecordedSession => ({
  id,
  connectionId: 'conn',
  connectionName: id,
  entryId: 'entry',
  entryName: 'Agent',
  createdAt: 100,
  endedAt: ended ? 200 : null,
  result: ended ? 'closed' : null,
  incomplete: false,
  lastSequence: 1,
  bytes: 10,
});
beforeEach(() => {
  f.runtime = { status: 'ready', instanceId: 'runtime' };
  f.request.mockReset();
  f.listeners.clear();
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })) as unknown as typeof window.matchMedia;
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  Object.assign(window, {
    vigilshell: {
      copyText: vi.fn(),
      mcp: {
        request: f.request,
        onChanged: (listener: (event: AgentEvent) => void) => {
          f.listeners.add(listener);
          return () => f.listeners.delete(listener);
        },
      },
    },
  });
  f.request.mockImplementation(async (request: AgentUiRequest) => {
    if (request.action === 'activitySources')
      return {
        ok: true,
        reply: { kind: 'activitySources', sources: [{ id: 'entry', client: 'codex' }] },
      };
    if (request.action === 'activitySummaries')
      return {
        ok: true,
        reply: {
          kind: 'activitySummaries',
          more: false,
          items: [
            {
              id: 't:task',
              entryId: 'entry',
              taskId: 'task',
              sessionId: 'live',
              sequence: null,
              createdAt: 100,
              connectionName: 'live',
              entryName: 'Agent',
              kind: 'command_run.command',
              summary: 'uname -a',
              phase: 'succeeded',
              startedAt: 100,
              finishedAt: 200,
              uncertain: false,
            },
          ],
        },
      };
    if (request.action === 'recordings')
      return {
        ok: true,
        reply: {
          kind: 'recordings',
          sessions: request.filter.sessionId
            ? [recording(request.filter.sessionId)]
            : [recording('live'), recording('ended', true)],
        },
      };
    if (request.action === 'task')
      return { ok: true, reply: { kind: 'task', task: task(request.taskId, 1, 'succeeded') } };
    throw Error(`Unexpected mutation: ${request.action}`);
  });
});
afterEach(cleanup);
it('shows active and ended sessions and never offers approval or execution controls', async () => {
  render(<AgentActivity onBack={() => {}} target={{ view: 'activity' }} />);
  expect(await screen.findByText('uname -a')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /批准|拒绝|停止|结束会话/ })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('tab', { name: '活跃' }));
  await waitFor(() =>
    expect(f.request).toHaveBeenCalledWith(
      expect.objectContaining({
        action: 'activitySummaries',
        filter: expect.objectContaining({ active: true }),
      }),
    ),
  );
  expect(
    f.request.mock.calls.every(([value]) =>
      ['activitySources', 'activitySummaries', 'recordings', 'activity'].includes(value.action),
    ),
  ).toBe(true);
});
it('keeps read failures distinct from empty state without refresh or mutations', async () => {
  f.request.mockResolvedValue({ ok: false, code: 'activity_recording_unavailable' });
  render(<AgentActivity onBack={() => {}} target={{ view: 'activity' }} />);
  await screen.findByRole('alert');
  expect(screen.queryByRole('button', { name: '刷新' })).not.toBeInTheDocument();
  expect(screen.queryByText('暂无活动记录')).not.toBeInTheDocument();
  expect(
    f.request.mock.calls.every(([value]) =>
      ['activitySources', 'activitySummaries'].includes(value.action),
    ),
  ).toBe(true);
});
it('locates notification tasks in the summary list without loading details', async () => {
  render(<AgentActivity onBack={() => {}} target={{ view: 'activity', taskId: 'older' }} />);
  expect(await screen.findByText('echo older')).toBeInTheDocument();
  expect(screen.queryByText('uname -a')).not.toBeInTheDocument();
  expect(
    f.request.mock.calls.every(([r]) =>
      ['activitySources', 'activitySummaries', 'task'].includes(r.action),
    ),
  ).toBe(true);
  await userEvent.click(screen.getByRole('tab', { name: '已结束' }));
  expect(await screen.findByText('uname -a')).toBeInTheDocument();
});
it('keeps field tooltips but removes detail columns and row navigation', async () => {
  render(<AgentActivity target={{ view: 'activity' }} />);
  const summary = await screen.findByText('uname -a');
  expect(screen.getAllByRole('columnheader')).toHaveLength(7);
  expect(screen.queryByRole('columnheader', { name: '详情' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '详情' })).not.toBeInTheDocument();
  const reads = f.request.mock.calls.length;
  await userEvent.click(summary);
  await userEvent.keyboard('{Enter}');
  expect(screen.getByRole('grid')).toBeVisible();
  expect(f.request).toHaveBeenCalledTimes(reads);
  await userEvent.hover(summary);
  expect(await screen.findByRole('region', { name: '操作摘要' })).toHaveTextContent('uname -a');
});

const task = (id: string, revision = 1, phase: AgentTask['phase'] = 'running'): AgentTask =>
  ({
    id,
    connectionId: 'conn',
    entryId: 'entry',
    cwd: '/',
    policyRevision: 1,
    parentTaskId: null,
    outputStart: 0,
    outputEnd: 0,
    terminal: false,
    terminationConfirmed: phase === 'succeeded',
    operation: null,
    error: null,
    exitCode: null,
    revision,
    contextId: 'live',
    connectionName: 'live',
    entryName: 'Agent',
    createdAt: 300,
    tool: 'command_run.command',
    command: `echo ${id}`,
    targets: [],
    phase,
    finishedAt: phase === 'succeeded' ? 500 : null,
    executionState: {
      phase: 'running',
      review: { kind: 'notRequired' },
      terminal: false,
      dispatch: { kind: 'attempted', attemptId: 'attempt', attemptedAt: 300 },
      outcome: null,
    },
  }) as AgentTask;
function emit(event: AgentEvent) {
  act(() => {
    for (const listener of f.listeners) listener(event);
  });
}
it('inserts task pushes and applies only newer revisions without reloading the list', async () => {
  render(<AgentActivity target={{ view: 'activity' }} />);
  await screen.findByText('uname -a');
  const reads = f.request.mock.calls.length;
  emit({ kind: 'task', task: task('new') });
  expect(await screen.findByText('echo new')).toBeInTheDocument();
  emit({ kind: 'task', task: task('new', 3, 'succeeded') });
  emit({ kind: 'task', task: task('new', 2, 'running') });
  expect(screen.queryByText('执行中')).not.toBeInTheDocument();
  expect(screen.getAllByText('echo new')).toHaveLength(1);
  expect(f.request).toHaveBeenCalledTimes(reads);
  expect(screen.queryByRole('heading', { name: '智能体活动' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '刷新' })).not.toBeInTheDocument();
});
it('preserves pushes received before the initial snapshot completes', async () => {
  let resolve!: (value: unknown) => void;
  const original = f.request.getMockImplementation()!;
  f.request.mockImplementation((request: AgentUiRequest) =>
    request.action === 'activitySummaries'
      ? new Promise((done) => {
          resolve = done;
        })
      : original(request),
  );
  render(<AgentActivity target={{ view: 'activity' }} />);
  emit({ kind: 'task', task: task('new', 3, 'succeeded') });
  await act(async () =>
    resolve({ ok: true, reply: { kind: 'activitySummaries', items: [], more: false } }),
  );
  expect(screen.getByText('echo new')).toBeInTheDocument();
  expect(f.request.mock.calls.filter(([r]) => r.action === 'activitySummaries')).toHaveLength(1);
});
it('removes finished tasks from the active filter and uses global empty state', async () => {
  f.request.mockResolvedValue({
    ok: true,
    reply: { kind: 'activitySummaries', items: [], more: false },
  });
  const view = render(<AgentActivity target={{ view: 'activity' }} />);
  await screen.findByText('暂无活动记录');
  expect(view.container.querySelector('[data-slot="empty-icon"]')).not.toBeNull();
  await userEvent.click(screen.getByRole('tab', { name: '活跃' }));
  await waitFor(() =>
    expect(f.request.mock.calls.filter(([r]) => r.action === 'activitySummaries')).toHaveLength(2),
  );
  emit({ kind: 'task', task: task('new') });
  expect(await screen.findByText('echo new')).toBeInTheDocument();
  emit({ kind: 'task', task: task('new', 2, 'succeeded') });
  expect(screen.queryByText('echo new')).not.toBeInTheDocument();
  expect(f.request.mock.calls.filter(([r]) => r.action === 'activitySummaries')).toHaveLength(2);
});
it('adds independent events once and ignores output fragments', async () => {
  render(<AgentActivity target={{ view: 'activity' }} />);
  await screen.findByText('uname -a');
  const record = {
    sessionId: 'live',
    taskId: null,
    parentTaskId: null,
    sequence: 2,
    timestampMs: 500,
    actor: 'user',
    policyRevision: 1,
    body: { kind: 'control', action: 'stop' },
  } as const;
  emit({ kind: 'activity', record });
  emit({ kind: 'activity', record });
  expect(screen.getAllByText('stop')).toHaveLength(1);
  emit({
    kind: 'activity',
    record: {
      ...record,
      sequence: 3,
      body: { kind: 'output', stream: 'stdout', text: 'output-fragment', data: null },
    },
  });
  expect(screen.queryByText('output-fragment')).not.toBeInTheDocument();
  expect(f.request.mock.calls.filter(([r]) => r.action === 'activitySummaries')).toHaveLength(1);
});

it('waits only for transport startup and loads once when the backend becomes ready', async () => {
  f.runtime = { status: 'starting', instanceId: null };
  const view = render(<AgentActivity target={{ view: 'activity' }} />);
  expect(f.request).not.toHaveBeenCalled();
  f.runtime = { status: 'ready', instanceId: 'runtime' };
  view.rerender(<AgentActivity target={{ view: 'activity' }} />);
  await screen.findByText('uname -a');
  expect(f.request.mock.calls.filter(([r]) => r.action === 'activitySummaries')).toHaveLength(1);
  f.runtime = { status: 'ready', instanceId: 'runtime' };
  view.rerender(<AgentActivity target={{ view: 'activity' }} />);
  expect(f.request.mock.calls.filter(([r]) => r.action === 'activitySummaries')).toHaveLength(1);
});

it('places the page close action inside the activity view without refreshing data', async () => {
  const close = vi.fn();
  const { container } = render(<AgentActivity target={{ view: 'activity' }} onBack={close} />);
  await screen.findByText('uname -a');
  const button = screen.getByRole('button', { name: '关闭活动页' });
  expect(container.querySelector('section')).toContainElement(button);
  expect(button.closest('.window-toolbar')).toBeNull();
  await userEvent.click(button);
  expect(close).toHaveBeenCalledOnce();
  expect(f.request.mock.calls.filter(([r]) => r.action === 'activitySummaries')).toHaveLength(1);
});

it('places agent identity first and uses the logo associated with its entry ID', async () => {
  const { container } = render(<AgentActivity target={{ view: 'activity' }} />);
  await screen.findByText('uname -a');
  expect(screen.getAllByRole('columnheader')[0]).toHaveTextContent('智能体');
  await waitFor(() => expect(container.querySelector('.mcp-client-logo')).not.toBeNull());
});

it('shows and copies the full multiline command without fetching details', async () => {
  const command =
    'journalctl -u payment-gateway --since "30 minutes ago" --no-pager\n' +
    'printf "%s\\n" "' +
    'long-argument-'.repeat(30) +
    '"';
  const original = f.request.getMockImplementation()!;
  f.request.mockImplementation(async (request: AgentUiRequest) => {
    const result = await original(request);
    if (request.action === 'activitySummaries') result.reply.items[0].summary = command;
    return result;
  });
  render(<AgentActivity target={{ view: 'activity' }} />);
  const trigger = await screen.findByText(
    (_, element) =>
      element?.getAttribute('data-slot') === 'hover-card-trigger' &&
      element.textContent === command,
  );
  const reads = f.request.mock.calls.length;
  await userEvent.hover(trigger);
  const popup = await screen.findByRole('region', { name: '操作摘要' });
  expect(popup.querySelector('pre')?.textContent).toBe(command);
  await userEvent.click(within(popup).getByRole('button', { name: '复制' }));
  expect(window.vigilshell.copyText).toHaveBeenCalledWith(command);
  expect(f.request).toHaveBeenCalledTimes(reads);
});

it.each([
  { uncertain: true },
  { phase: 'interruptedUnknown' },
  { startedAt: null },
  { finishedAt: null },
  { startedAt: 300, finishedAt: 100 },
  { phase: 'requestAccepted' },
])('renders an unavailable duration as a dash for bridge records: %j', async (fields) => {
  const original = f.request.getMockImplementation()!;
  f.request.mockImplementation(async (request: AgentUiRequest) => {
    const result = await original(request);
    if (request.action === 'activitySummaries') Object.assign(result.reply.items[0], fields);
    return result;
  });
  render(<AgentActivity target={{ view: 'activity' }} />);
  const summary = await screen.findByText('uname -a');
  const row = summary.closest('[role="row"]')! as HTMLElement;
  expect(within(row).getAllByRole('gridcell').at(-1)).toHaveTextContent(/^—$/);
});

it.each([
  [{ phase: 'running' }, '执行中', null],
  [{ phase: 'succeeded' }, '已完成', null],
  [{ phase: 'denied' }, '已拒绝', null],
  [{ phase: 'failed' }, '失败', null],
  [{ phase: 'timedOut' }, '失败', '已超时'],
  [{ phase: 'cancelled' }, '已取消', null],
  [{ phase: 'queued' }, '执行中', '排队中'],
  [{ phase: 'waitingReview' }, '执行中', '等待查看确认'],
  [{ phase: 'stopping' }, '执行中', '正在停止'],
  [{ phase: 'requestAccepted' }, '—', '请求已接受，实际结果待确认'],
  [{ phase: 'interruptedUnknown' }, '—', '已中断，结果未知'],
  [{ uncertain: true }, '—', '状态待确认'],
  [{ taskId: null, phase: null }, '—', null],
] as const)(
  'simplifies activity status without losing its context: %j',
  async (fields, label, note) => {
    const original = f.request.getMockImplementation()!;
    f.request.mockImplementation(async (request: AgentUiRequest) => {
      const result = await original(request);
      if (request.action === 'activitySummaries') Object.assign(result.reply.items[0], fields);
      return result;
    });
    render(<AgentActivity target={{ view: 'activity' }} />);
    const summary = await screen.findByText('uname -a');
    const row = summary.closest('[role="row"]')! as HTMLElement;
    expect(within(row).getAllByRole('gridcell')[3]?.textContent).toBe(label);
    if (note) expect(row).not.toHaveTextContent(note);
    const reads = f.request.mock.calls.length;
    await userEvent.hover(summary);
    const popup = await screen.findByRole('region', { name: '操作摘要' });
    if (note) expect(popup).toHaveTextContent(note);
    else expect(popup.querySelector('[data-slot="field-description"]')).toBeNull();
    await userEvent.click(within(popup).getByRole('button', { name: '复制' }));
    expect(window.vigilshell.copyText).toHaveBeenCalledWith('uname -a');
    expect(f.request).toHaveBeenCalledTimes(reads);
  },
);

it('keeps workflow context accessible even when the operation summary is empty', async () => {
  const original = f.request.getMockImplementation()!;
  f.request.mockImplementation(async (request: AgentUiRequest) => {
    const result = await original(request);
    if (request.action === 'activitySummaries')
      Object.assign(result.reply.items[0], { phase: 'requestAccepted', summary: '' });
    return result;
  });
  const { container } = render(<AgentActivity target={{ view: 'activity' }} />);
  await screen.findByText('Agent');
  await userEvent.hover(container.querySelector('[data-slot="hover-card-trigger"]')!);
  const popup = await screen.findByRole('region', { name: '操作摘要' });
  expect(popup).toHaveTextContent('请求已接受，实际结果待确认');
  expect(within(popup).queryByRole('button', { name: '复制' })).not.toBeInTheDocument();
});

it('keeps review in progress until the backend reports its final result', async () => {
  render(<AgentActivity target={{ view: 'activity' }} />);
  await screen.findByText('uname -a');
  const reads = f.request.mock.calls.length;
  emit({ kind: 'task', task: task('review', 1, 'waitingReview') });
  const summary = await screen.findByText('echo review');
  const status = () =>
    within(summary.closest('[role="row"]')! as HTMLElement).getAllByRole('gridcell')[3];
  expect(status()).toHaveTextContent('执行中');
  emit({ kind: 'task', task: task('review', 2, 'timedOut') });
  expect(status()).toHaveTextContent('失败');
  emit({ kind: 'task', task: task('review', 1, 'waitingReview') });
  expect(status()).toHaveTextContent('失败');
  expect(f.request).toHaveBeenCalledTimes(reads);
});

it('reads the desktop activity source in development mode without preview records', async () => {
  vi.stubEnv('DEV', true);
  vi.stubEnv('MODE', 'development');
  try {
    render(<AgentActivity target={{ view: 'activity' }} />);
    expect(await screen.findByText('uname -a')).toBeInTheDocument();
    expect(f.request).toHaveBeenCalledWith({ action: 'activitySummaries', filter: {} });
    expect(screen.queryByText('预发布应用服务器')).not.toBeInTheDocument();
    emit({ kind: 'task', task: task('live-development') });
    expect(await screen.findByText('echo live-development')).toBeInTheDocument();
  } finally {
    vi.unstubAllEnvs();
  }
});
