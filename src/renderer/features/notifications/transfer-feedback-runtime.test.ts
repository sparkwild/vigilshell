// @vitest-environment jsdom
import { expect, it, vi } from 'vitest';
import type { WorkspaceStore } from '../connections/workspace-store';
import { startTransferFeedback } from './transfer-feedback-runtime';
it('reports actual visible transfer bodies without a notification component and retires listeners', () => {
  const focused = vi.spyOn(document, 'hasFocus').mockReturnValue(true);
  const report = vi.fn();
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: { reportTransferFeedback: report },
  });
  let changed = () => {};
  const detach = vi.fn();
  const task = { id: 'task', attempt: 0, phase: 'failed', conflictVersion: null };
  const anchors = new Set<string>();
  const workspace = {
    getSnapshot: () => ({ runtimeId: 'runtime', transfers: new Map([['task', task]]) }),
    subscribe: () => detach,
    feedback: {
      getSnapshot: () => ({ anchors }),
      subscribe: (listener: () => void) => {
        changed = listener;
        return detach;
      },
    },
  } as unknown as WorkspaceStore;
  const stop = startTransferFeedback(workspace);
  expect(report).not.toHaveBeenCalled();
  anchors.add('transfer:task:0:failed:null');
  changed();
  changed();
  expect(report).toHaveBeenCalledExactlyOnceWith({
    runtimeId: 'runtime',
    keys: ['transfer:task:0:failed:null'],
  });
  task.attempt = 1;
  anchors.add('transfer:task:1:failed:null');
  changed();
  expect(report).toHaveBeenCalledTimes(2);
  stop();
  window.dispatchEvent(new Event('focus'));
  expect(detach).toHaveBeenCalledTimes(2);
  expect(report).toHaveBeenCalledTimes(2);
  focused.mockRestore();
});
