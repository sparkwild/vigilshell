// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { AgentSessionView } from '@contracts';
import { AgentSessionPending } from './AgentSessionPending';
import { translate } from '../../../shared/i18n/messages';

const issue = vi.hoisted(() => vi.fn());
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: Parameters<typeof translate>[0]) => translate(key, 'zh-CN') }),
}));
vi.mock('./model', () => ({ useMcp: () => ({ busy: false, error: null, issue }) }));
vi.mock('../terminal/AgentControlNotice', () => ({
  ControlledAgentLogo: () => <span>Cursor Logo</span>,
}));
vi.mock('../notifications/NotificationFeedbackProvider', () => ({
  useNotificationSurface: () => undefined,
}));
afterEach(() => {
  cleanup();
  issue.mockClear();
});

it.each(['authorization_expired', 'connection_failed'])(
  'keeps the same floating notice and error after %s',
  (error) => {
    const pending = {
      runningTasks: 0,
      context: {
        id: 'new',
        phase: 'waitingUser',
        entryId: 'cursor',
        entryName: 'Cursor',
        connectionName: 'Server',
      },
    } as AgentSessionView;
    const view = render(<AgentSessionPending view={pending} visible />);
    const floating = document.querySelector('.terminal-floating-notice');
    expect(screen.getByRole('button', { name: '验证并应用' })).toBeInTheDocument();
    view.rerender(
      <AgentSessionPending
        view={{ ...pending, context: { ...pending.context, phase: 'failed', error } }}
        visible
      />,
    );
    expect(document.querySelector('.terminal-floating-notice')).toBe(floating);
    expect(screen.getByRole('status')).toHaveTextContent('Cursor');
    expect(screen.getByRole('status')).toHaveTextContent('会话失败');
    expect(screen.getByRole('alert')).toBeVisible();
    if (error === 'authorization_expired')
      expect(screen.getByRole('alert')).toHaveTextContent('已过期');
    expect(screen.queryByRole('button', { name: '验证并应用' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '解除控制' })).not.toBeInTheDocument();
    expect(screen.queryByText('执行步骤')).not.toBeInTheDocument();
    expect(issue).not.toHaveBeenCalled();
  },
);
