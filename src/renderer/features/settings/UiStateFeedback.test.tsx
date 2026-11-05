// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { UiStateFeedback } from './UiStateFeedback';
import { translate } from '../../../shared/i18n/messages';
const f = vi.hoisted(() => ({ scopes: ['connection:one', 'pickers'], retry: vi.fn() }));
vi.mock('../../shared/platform/ui-state-store', () => ({
  useUiStateErrors: () => f.scopes,
  retryUiState: f.retry,
}));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: Parameters<typeof translate>[0]) => translate(key, 'en') }),
}));
afterEach(() => {
  cleanup();
  f.retry.mockClear();
});
it('shows only the current functional scope and retries it explicitly', async () => {
  const view = render(<UiStateFeedback scope="connection:two" />);
  expect(screen.queryByRole('alert')).toBeNull();
  view.rerender(<UiStateFeedback scope="connection:one" />);
  expect(screen.getByRole('alert')).toBeVisible();
  expect(f.retry).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
  expect(f.retry).toHaveBeenCalledExactlyOnceWith('connection:one');
  view.rerender(<UiStateFeedback scope="pickers" />);
  await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
  expect(f.retry).toHaveBeenLastCalledWith('pickers');
});
