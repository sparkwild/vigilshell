// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { AgentTask } from '@contracts';
import { ScriptReview } from './ScriptReview';
import { translate, type MessageKey } from '../../../shared/i18n/messages';
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: MessageKey) => translate(key, 'en') }),
}));
afterEach(cleanup);
it.each([
  'authorization_cancelled',
  'authorization_failed',
  'authorization_timed_out',
  'authorization_interrupted',
  'configuration_conflict',
])('keeps script review pending after an unsuccessful approval: %s', async (code) => {
  const request = vi.fn(async () => ({ ok: false, code }));
  Object.defineProperty(window, 'vigilshell', { configurable: true, value: { mcp: { request } } });
  const task = {
    id: 'review',
    phase: 'waitingReview',
    scriptSnapshot: {
      text: 'printf FULL_CONTENT',
      path: '/srv/source.sh',
      directory: '/srv',
      hash: 'a'.repeat(64),
      interpreter: 'bash',
      arguments: ['a b'],
    },
  } as AgentTask;
  render(<ScriptReview task={task} />);
  expect(screen.getByText('printf FULL_CONTENT')).toBeInTheDocument();
  expect(screen.getByText(/\/srv\/source.sh/)).toBeInTheDocument();
  expect(screen.queryByText(/SHA-256/)).not.toBeInTheDocument();
  expect(request).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Confirm execution' }));
  await waitFor(() => expect(request).toHaveBeenCalledOnce());
  expect(request).toHaveBeenCalledWith({ action: 'approveScript', taskId: 'review' });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Confirm execution' })).toBeEnabled(),
  );
  if (code === 'configuration_conflict') expect(screen.getByRole('alert')).toBeInTheDocument();
  else expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  expect(task.phase).toBe('waitingReview');
  fireEvent.click(screen.getByRole('button', { name: 'Do not execute' }));
  await waitFor(() =>
    expect(request).toHaveBeenLastCalledWith({ action: 'rejectScript', taskId: 'review' }),
  );
});
