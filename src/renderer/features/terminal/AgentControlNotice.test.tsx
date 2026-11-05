// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { AgentControlNotice } from './AgentControlNotice';
import { translate } from '../../../shared/i18n/messages';
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], params?: Record<string, string | number>) =>
      translate(key, 'zh-CN', params),
  }),
}));
vi.mock('../mcp/AgentWorkspaceReview', () => ({ useAgentWorkspaceReview: () => null }));
vi.mock('../mcp/model', () => ({ useMcp: () => ({ snapshot: { entries: [] } }) }));
beforeEach(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
const control = {
  entryId: 'entry',
  entryName: 'DeepSeek Harness',
  contextId: 'context',
  leaseId: 'lease',
  releasing: false,
};
it('shows the actual agent and only releases its current session without an extra confirmation', async () => {
  const request = vi.fn(async () => ({ ok: true }));
  Object.assign(window, { vigilshell: { mcp: { request } } });
  const view = render(<AgentControlNotice control={control} />);
  expect(screen.getByRole('status')).toHaveTextContent('DeepSeek Harness 正在操作');
  expect(request).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: '解除控制' }));
  await waitFor(() =>
    expect(request).toHaveBeenCalledExactlyOnceWith({
      action: 'closeSession',
      sessionId: 'context',
    }),
  );
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  // Only the authoritative snapshot removes the lock, not a successful request alone.
  expect(screen.getByRole('status')).toBeInTheDocument();
  view.rerender(<AgentControlNotice control={{ ...control, releasing: true }} />);
  expect(screen.getByRole('status')).toHaveTextContent('正在解除 DeepSeek Harness 的控制');
});
it('keeps the release action available after a failed handover', async () => {
  const request = vi.fn(async () => ({ ok: false, code: 'terminal_control_busy' }));
  Object.assign(window, { vigilshell: { mcp: { request } } });
  render(<AgentControlNotice control={control} />);
  await userEvent.click(screen.getByRole('button', { name: '解除控制' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '解除控制' })).toBeEnabled());
  expect(screen.getByRole('status')).toHaveTextContent('DeepSeek Harness 正在操作');
  expect(screen.getByRole('alert')).toBeInTheDocument();
});
