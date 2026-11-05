import { gzipSync } from 'node:zlib';
// @vitest-environment node
import { expect, it, vi } from 'vitest';
import type { ActivityRecord, AgentOperation, AgentReply, AgentTask } from '@contracts';
import { AgentScreens } from './agent-screens';
const task: AgentTask = {
  id: 'terminal',
  contextId: 'session',
  entryId: 'entry',
  entryName: 'Configured',
  connectionId: 'connection',
  connectionName: 'Server',
  connectionRevision: 1,
  command: 'terminal',
  cwd: '/srv/app',
  policyRevision: 1,
  targets: [],
  phase: 'succeeded',
  createdAt: 1,
  finishedAt: 2,
  exitCode: 0,
  error: null,
  terminationConfirmed: true,
  terminal: true,
  tool: 'terminal_manage.open',
  parentTaskId: null,
  outputStart: 0,
  outputEnd: 4,
  operation: { kind: 'terminal', directory: { scopeId: 'work', path: '.' }, columns: 80, rows: 24 },
};
const event = (sequence: number, body: ActivityRecord['body']): ActivityRecord => ({
  sessionId: 'session',
  taskId: task.id,
  parentTaskId: null,
  sequence,
  timestampMs: 1,
  actor: 'agent',
  policyRevision: 1,
  body,
});

it('changes display scrollback without executing or truncating stored output', async () => {
  const text = Array.from({ length: 1800 }, (_, i) => `ROW-${String(i).padStart(4, '0')}\r\n`).join(
    '',
  );
  const records = [event(1, { kind: 'output', stream: 'terminal', text, data: null })];
  const request = vi.fn(async (operation: AgentOperation): Promise<AgentReply> => {
    if (operation.operation === 'task') return { kind: 'task', task: { ...task, outputEnd: 1 } };
    if (operation.operation === 'loadScreen') return { kind: 'screen', checkpoint: null };
    if (operation.operation === 'activity')
      return {
        kind: 'activity',
        page: {
          records: records.filter((r) => r.sequence > operation.cursor),
          nextCursor: 1,
          incomplete: false,
        },
      };
    throw Error('Changing display settings must not mutate execution or recording');
  });
  let rows = 5000;
  const screens = new AgentScreens(request, () => rows);
  expect((await screens.snapshot(task.id)).ansi).toContain('ROW-0000');
  rows = 1000;
  screens.refreshPreferences();
  const shortened = await screens.snapshot(task.id);
  expect(shortened.ansi).not.toContain('ROW-0000');
  expect(shortened.ansi).toContain('ROW-1799');
  expect(records[0]!.body).toMatchObject({ text });
  expect(
    request.mock.calls.every(([op]) => ['task', 'activity', 'loadScreen'].includes(op.operation)),
  ).toBe(true);
  await screens.close();
});
it('restores real terminal clearing, dimensions and cursor state using read-only records', async () => {
  const records = [
    event(1, { kind: 'output', stream: 'terminal', text: '中文\x1b[31mred\x1b[0m', data: null }),
    event(2, { kind: 'resize', columns: 20, rows: 10 }),
    event(3, { kind: 'output', stream: 'terminal', text: '\x1b[2J\x1b[HFINAL', data: null }),
  ];
  const request = vi.fn(async (operation: AgentOperation): Promise<AgentReply> => {
    if (operation.operation === 'task') return { kind: 'task', task };
    if (operation.operation === 'activity')
      return {
        kind: 'activity',
        page: {
          records: records.filter((record) => record.sequence > operation.cursor),
          nextCursor: 3,
          incomplete: false,
        },
      };
    throw Error('Replay must not invoke execution');
  });
  const screens = new AgentScreens(request, () => 10000);
  const snapshot = await screens.snapshot(task.id);
  expect(snapshot.columns).toBe(20);
  expect(snapshot.rows).toBe(10);
  expect(snapshot.ansi).toContain('FINAL');
  expect(snapshot.ansi).not.toContain('中文');
  expect(snapshot.incomplete).toBe(false);
  expect(
    request.mock.calls.every(([operation]) =>
      ['task', 'activity', 'loadScreen'].includes(operation.operation),
    ),
  ).toBe(true);
  await screens.close();
});
it('a slow live display catches up from storage instead of permanently invalidating its screen', async () => {
  const records = Array.from({ length: 48 }, (_, index) =>
    event(index + 1, {
      kind: 'output',
      stream: 'terminal',
      text: 'x'.repeat(32768) + (index === 47 ? '\r\nFINISHED' : ''),
      data: null,
    }),
  );
  const request = vi.fn(async (operation: AgentOperation): Promise<AgentReply> => {
    if (operation.operation !== 'activity') throw Error('Only observation is allowed');
    const page = records.filter((record) => record.sequence > operation.cursor).slice(0, 4);
    return {
      kind: 'activity',
      page: {
        records: page,
        nextCursor: page.at(-1)?.sequence ?? operation.cursor,
        incomplete: false,
      },
    };
  });
  const screens = new AgentScreens(request, () => 1000);
  screens.event({ kind: 'task', task: { ...task, phase: 'running', outputEnd: 48 } });
  for (const record of records) screens.event({ kind: 'activity', record });
  let snapshot = await screens.snapshot(task.id);
  for (let tries = 0; snapshot.sequence < 48 && tries < 10; tries++)
    snapshot = await screens.snapshot(task.id);
  expect(request).toHaveBeenCalled();
  expect(snapshot.sequence).toBe(48);
  expect(snapshot.ansi).toContain('FINISHED');
  expect(snapshot.incomplete).toBe(false);
  await screens.close();
});

it('resumes a persisted screen checkpoint and only replays later records', async () => {
  const request = vi.fn(async (operation: AgentOperation): Promise<AgentReply> => {
    if (operation.operation === 'task') return { kind: 'task', task };
    if (operation.operation === 'loadScreen')
      return {
        kind: 'screen',
        checkpoint: {
          version: 1,
          sequence: 2,
          columns: 80,
          rows: 24,
          gzip: gzipSync('BEFORE').toString('base64'),
        },
      };
    if (operation.operation === 'activity') {
      expect(operation.cursor).toBe(2);
      return {
        kind: 'activity',
        page: {
          records: [event(3, { kind: 'output', stream: 'terminal', text: ' AFTER', data: null })],
          nextCursor: 3,
          incomplete: false,
        },
      };
    }
    throw Error('Read-only restoration must not execute or write');
  });
  const screens = new AgentScreens(request, () => 10000);
  expect((await screens.snapshot(task.id)).ansi).toContain('BEFORE AFTER');
  await screens.close();
});
it('falls back to complete records when the cached checkpoint is damaged', async () => {
  const request = vi.fn(async (operation: AgentOperation): Promise<AgentReply> => {
    if (operation.operation === 'task') return { kind: 'task', task };
    if (operation.operation === 'loadScreen')
      return {
        kind: 'screen',
        checkpoint: { version: 1, sequence: 2, columns: 80, rows: 24, gzip: 'broken' },
      };
    if (operation.operation === 'activity') {
      expect(operation.cursor).toBe(0);
      return {
        kind: 'activity',
        page: {
          records: [
            event(3, { kind: 'output', stream: 'terminal', text: 'RECOVERED', data: null }),
          ],
          nextCursor: 3,
          incomplete: false,
        },
      };
    }
    throw Error('Read-only restoration must not execute or write');
  });
  const screens = new AgentScreens(request, () => 10000);
  expect((await screens.snapshot(task.id)).ansi).toContain('RECOVERED');
  await screens.close();
});
