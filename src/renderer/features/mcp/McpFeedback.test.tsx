// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { translate, type MessageKey } from '../../../shared/i18n/messages';
import { McpFeedback } from './McpFeedback';

vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: MessageKey) => translate(key, 'zh-CN') }),
}));
afterEach(cleanup);
it('shows a useful error without exposing code or an expandable raw exception', () => {
  const view = render(<McpFeedback code="file_partial_write" />);
  expect(screen.getByRole('alert')).toHaveTextContent('部分内容可能已经写入');
  expect(view.container).not.toHaveTextContent('file_partial_write');
  expect(view.container.querySelector('details')).toBeNull();
  view.rerender(<McpFeedback code="Error: access token in unexpected response" />);
  expect(view.container).not.toHaveTextContent('access token');
  expect(screen.getByRole('alert')).toHaveTextContent('操作未能完成');
});
it('keeps the existing reload action for a configuration conflict', () => {
  const retry = vi.fn();
  render(<McpFeedback code="configuration_conflict" onRetry={retry} />);
  fireEvent.click(screen.getByRole('button'));
  expect(retry).toHaveBeenCalledOnce();
});
