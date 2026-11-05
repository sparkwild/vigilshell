// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { AgentEvent, AgentTask } from '@contracts';
import { useAgentWorkspaceReview } from './AgentWorkspaceReview';
vi.mock('../notifications/NotificationFeedbackProvider', () => ({
  useNotificationSurface: () => undefined,
}));
vi.mock('./ScriptReview', () => ({
  ScriptReview: ({ task }: { task: AgentTask }) => <div>{task.id}</div>,
}));
vi.mock('./McpFeedback', () => ({ McpFeedback: () => null }));
afterEach(cleanup);
function Review() {
  return useAgentWorkspaceReview({ sessionId: 'session' });
}
it('keeps script review available in the ordinary workspace and ignores an older snapshot', async () => {
  let receive!: (event: AgentEvent) => void;
  let resolve!: (value: unknown) => void;
  const unsubscribe = vi.fn();
  Object.assign(window, {
    vigilshell: {
      mcp: {
        onChanged: (listener: typeof receive) => {
          receive = listener;
          return unsubscribe;
        },
        request: vi.fn(
          () =>
            new Promise((done) => {
              resolve = done;
            }),
        ),
      },
    },
  });
  const view = render(<Review />);
  const task = {
    id: 'review-script',
    contextId: 'session',
    phase: 'waitingReview',
    revision: 2,
  } as AgentTask;
  act(() => receive({ kind: 'task', task }));
  expect(screen.getByText('review-script')).toBeVisible();
  await act(async () =>
    resolve({
      ok: true,
      reply: { kind: 'tasks', tasks: [{ ...task, phase: 'queued', revision: 1 }] },
    }),
  );
  expect(screen.getByText('review-script')).toBeVisible();
  act(() => receive({ kind: 'task', task: { ...task, phase: 'running', revision: 3 } }));
  expect(screen.queryByText('review-script')).not.toBeInTheDocument();
  view.unmount();
  expect(unsubscribe).toHaveBeenCalledOnce();
});
