// @vitest-environment jsdom
import { cleanup, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import type { TerminalSessionView } from '@contracts';
import { translate, type MessageKey } from '../../../shared/i18n/messages';
import { ConnectionFailure, connectionAdvice } from './ConnectionFailure';
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: MessageKey) => translate(key, 'zh-CN') }),
}));
const showInfo = vi.hoisted(() => vi.fn());
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({
    store: {
      feedback: { setAnchor: vi.fn() },
      editConnection: vi.fn(async () => {}),
      report: vi.fn(),
    },
  }),
  errorCode: () => 'unknown',
  errorKey: (code: string) => `ws_error_${code}`,
  phaseKey: (phase: string) => `ws_${phase}`,
}));
const session = {
  id: 'session-1',
  operationId: 'operation-1',
  connectionId: 'connection-1',
  username: 'tester',
  host: 'server.example',
  port: 22,
  phase: 'failed',
  error: 'disconnected',
  exitCode: null,
} as TerminalSessionView;
afterEach(cleanup);
it.each([
  'vault_key_missing',
  'vault_key_invalid',
  'credential_corrupt',
  'credential_format_unsupported',
])('explains %s as a vault problem rather than an SSH password failure', (error) => {
  render(<ConnectionFailure session={{ ...session, error }} />);
  expect(connectionAdvice(error)).toBe('connection_help_vault');
  expect(screen.getByText(/重新认证或输入 SSH 密码不能修复/)).toBeInTheDocument();
});
it('explains an unexpected disconnect and copies user-facing problem details', async () => {
  const write = vi.fn<(text: string) => Promise<void>>().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: write },
  });
  render(<ConnectionFailure session={session} />);
  expect(screen.getByText('SSH 连接意外中断')).toBeInTheDocument();
  expect(screen.queryByText('SSH 连接已结束')).not.toBeInTheDocument();
  fireEvent.click(screen.getByText('查看原因与处理建议'));
  fireEvent.click(screen.getByRole('button', { name: '复制问题说明' }));
  await waitFor(() => expect(write).toHaveBeenCalled());
  expect(write.mock.calls[0]?.[0]).not.toContain('session-1');
  expect(write.mock.calls[0]?.[0]).not.toContain('disconnected');
  expect(write.mock.calls[0]?.[0]).toContain('tester@server.example:22');
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('已复制'));
});
it.each(['running'] as const)('does not label %s as a failed connection', (phase) => {
  const { container } = render(<ConnectionFailure session={{ ...session, phase }} />);
  expect(container).toBeEmptyDOMElement();
});
it('selects actionable guidance without claiming a bad password', () => {
  render(<ConnectionFailure session={{ ...session, error: 'authentication_failed' }} />);
  expect(screen.getByText(/不一定是密码错误/)).toBeInTheDocument();
  expect(connectionAdvice('connection_refused')).toBe('connection_help_refused');
  expect(connectionAdvice('pty_rejected')).toBe('connection_help_terminal');
  expect(connectionAdvice('credential_denied')).toBe('ws_keychain_help');
  expect(connectionAdvice('unrecognized_code')).toBe('connection_help_unknown');
});

it('opens sidebar diagnostics by props without reactivating the floating popup', () => {
  showInfo.mockClear();
  const view = render(<ConnectionFailure session={session} />);
  expect(view.container.querySelector('details')).not.toHaveAttribute('open');
  view.rerender(<ConnectionFailure session={session} expanded revealToken={1} />);
  expect(view.container.querySelector('details')).toHaveAttribute('open');
  expect(showInfo).not.toHaveBeenCalled();
});
it('explains unknown remote closure without calling it a timeout, and treats remote text as text', () => {
  const view = render(
    <ConnectionFailure
      session={{
        ...session,
        phase: 'closed',
        error: null,
        endReason: { kind: 'remoteClosed', message: null },
      }}
    />,
  );
  expect(screen.getByText('远端已关闭连接，未提供具体原因')).toBeInTheDocument();
  view.rerender(
    <ConnectionFailure
      session={{
        ...session,
        phase: 'closed',
        error: null,
        endReason: { kind: 'remoteClosed', message: '<img src=x>' },
      }}
    />,
  );
  expect(view.container.querySelector('img')).toBeNull();
  expect(screen.queryByText('<img src=x>')).not.toBeInTheDocument();
});

it.each([
  ['closed', 'userDisconnected', null],
  ['cancelled', 'cancelled', null],
  ['closed', 'remoteExit', 0],
] as const)('does not show diagnostics for %s / %s', (phase, kind, exitCode) => {
  const { container } = render(
    <ConnectionFailure
      session={{ ...session, phase, error: null, exitCode, endReason: { kind, message: null } }}
      expanded
    />,
  );
  expect(container).toBeEmptyDOMElement();
  expect(screen.queryByText('查看原因与处理建议')).not.toBeInTheDocument();
});
