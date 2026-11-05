// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { OperationIndicator } from './OperationFeedback';
const f = vi.hoisted(() => ({
  op: {
    id: 'op',
    owner: 'connection:c',
    action: 'ws_connect',
    label: 'server',
    phase: 'failed',
    visible: true,
    error: { code: 'disconnected' },
    job: { kind: 'connect', sessionId: 'failed-session' },
  },
  sessions: [
    { id: 'failed-session', phase: 'failed' },
    { id: 'online-session', phase: 'running' },
  ],
  dismiss: vi.fn(),
  tab: vi.fn(),
}));
vi.mock('./ConnectionProvider', () => ({
  useConnections: () => ({
    sessions: f.sessions,
    snapshot: { appearance: { locale: 'zh-CN' } },
    store: { showTerminalDiagnostics: f.tab },
    operations: {
      forOwner: () => f.op,
      setAnchor: vi.fn(),
      dismiss: f.dismiss,
    },
  }),
}));
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  f.sessions = [
    { id: 'failed-session', phase: 'failed' },
    { id: 'online-session', phase: 'running' },
  ];
  f.tab.mockClear();
  f.dismiss.mockClear();
});
afterEach(cleanup);
async function open() {
  render(<OperationIndicator owner="connection:c" icon={<span>SSH</span>} />);
  const user = userEvent.setup();
  screen.getByRole('button').focus();
  await user.keyboard('{Enter}');
  await screen.findByText('SSH 连接意外中断');
  return user;
}
it('uses the shared failure title and navigates only to the failed session', async () => {
  const route = vi.fn();
  window.addEventListener('vigilshell:terminal-diagnostics', route, { once: true });
  const user = await open();
  expect(screen.queryByText('SSH 连接已结束')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '查看原因与处理建议' }));
  expect(f.tab).not.toHaveBeenCalled();
  expect(route).toHaveBeenCalledWith(expect.objectContaining({ detail: 'failed-session' }));
});
it('dismisses feedback without closing or changing a session', async () => {
  const user = await open();
  await user.click(screen.getByRole('button', { name: '忽略提示' }));
  expect(f.dismiss).toHaveBeenCalledWith('op');
  expect(f.tab).not.toHaveBeenCalled();
});
it('does not offer navigation to a session that has been closed', async () => {
  f.sessions = [{ id: 'online-session', phase: 'running' }];
  await open();
  expect(screen.queryByRole('button', { name: '查看原因与处理建议' })).not.toBeInTheDocument();
});
