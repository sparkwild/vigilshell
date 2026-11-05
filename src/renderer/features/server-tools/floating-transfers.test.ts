import { expect, it } from 'vitest';
import type { TransferView } from '@contracts';
import {
  floatingCandidates,
  transferBatchKey,
  transferIsCovered,
  updateFloatingState,
  type FloatingState,
} from './floating-transfers';
const task = (id: string, phase = 'running', batchId = id) =>
  ({ id, phase, batchId, connectionId: 'connection' }) as TransferView;
const initial: FloatingState = { runtimeId: null, batches: new Map() };
const hidden = new Set<string>();
it('only counts the matching connection and matching notification batch as covered', () => {
  const t = task('a');
  expect(transferIsCovered(t, 'runtime', 'connection', undefined)).toBe(true);
  expect(transferIsCovered(t, 'runtime', 'other', 'other')).toBe(false);
  expect(transferIsCovered(t, 'runtime', null, 'connection')).toBe(true);
  expect(
    transferIsCovered(t, 'runtime', null, 'connection', {
      runtimeId: 'runtime',
      batchId: 'other',
      connectionId: 'connection',
      phase: 'failed',
    }),
  ).toBe(false);
  expect(
    transferIsCovered(t, 'runtime', null, 'connection', {
      runtimeId: 'old',
      batchId: 'a',
      connectionId: 'connection',
      phase: 'failed',
    }),
  ).toBe(false);
});
it('shows uncovered activity, suppresses covered views, and returns when they close', () => {
  const tasks = [task('a')];
  let state = updateFloatingState(initial, 'runtime', tasks, new Set(['a']), 0);
  expect(floatingCandidates(tasks, state, new Set(['a']), 0)).toEqual([]);
  state = updateFloatingState(state, 'runtime', tasks, hidden, 1);
  expect(floatingCandidates(tasks, state, hidden, 1)).toEqual(tasks);
  state = updateFloatingState(state, 'runtime', tasks, new Set(['a']), 2);
  expect(floatingCandidates(tasks, state, new Set(['a']), 2)).toEqual([]);
  state = updateFloatingState(state, 'runtime', tasks, hidden, 3);
  expect(floatingCandidates(tasks, state, hidden, 3)).toEqual(tasks);
});
it('retains success briefly without replaying loaded history', () => {
  let tasks = [task('a'), task('historical', 'complete')];
  let state = updateFloatingState(initial, 'runtime', tasks, hidden, 0);
  expect(floatingCandidates(tasks, state, hidden, 0).map((t) => t.id)).toEqual(['a']);
  tasks = [task('a', 'complete'), task('historical', 'complete')];
  state = updateFloatingState(state, 'runtime', tasks, hidden, 100);
  expect(floatingCandidates(tasks, state, hidden, 4099).map((t) => t.id)).toEqual(['a']);
  expect(floatingCandidates(tasks, state, hidden, 4100)).toEqual([]);
  state = updateFloatingState(state, 'new-runtime', tasks, hidden, 4101);
  expect(floatingCandidates(tasks, state, hidden, 4101)).toEqual([]);
});
it('retains newly observed failures, but does not reopen dismissed batches for revisions or new tasks in that batch', () => {
  let tasks = [task('a')];
  let state = updateFloatingState(initial, 'runtime', tasks, hidden, 0);
  tasks = [task('a', 'failed')];
  state = updateFloatingState(state, 'runtime', tasks, hidden, 100);
  expect(floatingCandidates(tasks, state, hidden, 10000)).toEqual(tasks);
  state = {
    ...state,
    batches: new Map(
      [...state.batches].map(([key, batch]) => [key, { ...batch, dismissed: true }]),
    ),
  };
  tasks = [task('a', 'failed'), task('b', 'running', 'a'), task('new')];
  state = updateFloatingState(state, 'runtime', tasks, hidden, 10001);
  expect(floatingCandidates(tasks, state, hidden, 10001).map((t) => t.id)).toEqual(['new']);
  state = updateFloatingState(state, 'runtime', [], hidden, 10002);
  expect(state.batches.size).toBe(0);
});
it('does not replay resolved results after they were displayed in a full list', () => {
  let state = updateFloatingState(initial, 'runtime', [task('a')], hidden, 0);
  const tasks = [task('a', 'failed')];
  state = updateFloatingState(state, 'runtime', tasks, new Set(['a']), 1);
  state = updateFloatingState(state, 'runtime', tasks, hidden, 2);
  expect(floatingCandidates(tasks, state, hidden, 2)).toEqual([]);
});
it('does not allocate new presentation state for progress-only updates', () => {
  const tasks = [task('a')];
  const state = updateFloatingState(initial, 'runtime', tasks, hidden, 0);
  expect(updateFloatingState(state, 'runtime', [{ ...tasks[0]!, bytes: 100 }], hidden, 1)).toBe(
    state,
  );
  expect(state.batches.get(transferBatchKey(tasks[0]!))?.active).toBe(true);
});
it('does not repeat the conflict dialog as another automatic floating reminder', () => {
  const tasks = [task('a', 'awaitingDecision')];
  const state = updateFloatingState(initial, 'runtime', tasks, hidden, 0);
  expect(floatingCandidates(tasks, state, hidden, 0)).toEqual([]);
});
