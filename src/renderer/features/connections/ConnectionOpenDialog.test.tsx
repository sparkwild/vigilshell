// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { translate } from '../../../shared/i18n/messages';
import { ConnectionOpenDialog } from './ConnectionOpenDialog';
import type { ConnectionOpenChoice, ConnectionOpenView } from './connection-open-controller';
import { WorkspaceUiContext, useOpenConnection, type WorkspaceUiActions } from './ui-context';
const language = vi.hoisted(() => ({ locale: 'zh-CN' as 'zh-CN' | 'en' }));
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], inputs?: Record<string, string | number>) =>
      translate(key, language.locale, inputs),
  }),
}));
afterEach(cleanup);
beforeEach(() => {
  language.locale = 'zh-CN';
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn().mockImplementation(() => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
const view: ConnectionOpenView = {
  connectionId: 'id',
  name: '<img src=x>',
  count: 2,
  newBlocked: null,
};
it('renders a neutral three-way choice, escapes the name, focuses cancel, and Escape cancels', async () => {
  const result = vi.fn();
  function Harness() {
    const [current, setCurrent] = useState<ConnectionOpenView | null>(view);
    return (
      <ConnectionOpenDialog
        view={current}
        choose={(choice) => {
          result(choice);
          setCurrent(null);
        }}
      />
    );
  }
  render(<Harness />);
  const dialog = screen.getByRole('alertdialog');
  expect(within(dialog).getByText(/<img src=x>/)).toBeVisible();
  expect(dialog.querySelector('img')).toBeNull();
  expect(within(dialog).getAllByRole('button')).toHaveLength(3);
  await waitFor(() => expect(within(dialog).getByRole('button', { name: '取消' })).toHaveFocus());
  await userEvent.setup().keyboard('{Escape}');
  await waitFor(() => expect(result).toHaveBeenCalledExactlyOnceWith('cancel'));
  await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument());
});
it.each([
  ['切换到已有终端', 'reuse'],
  ['新建终端', 'new'],
] as const)('%s produces the explicit %s action', async (label, action) => {
  const choose = vi.fn<(choice: ConnectionOpenChoice) => void>();
  render(<ConnectionOpenDialog view={view} choose={choose} />);
  await userEvent.setup().click(screen.getByRole('button', { name: label }));
  expect(choose).toHaveBeenCalledExactlyOnceWith(action);
});
it('disables only creation at capacity and updates localized text and counts', async () => {
  const choose = vi.fn();
  const rendered = render(
    <ConnectionOpenDialog
      view={{ ...view, count: 4, newBlocked: 'connection_limit' }}
      choose={choose}
    />,
  );
  expect(screen.getByRole('button', { name: '新建终端' })).toBeDisabled();
  expect(screen.getByRole('button', { name: '切换到已有终端' })).toBeEnabled();
  expect(screen.getByRole('status')).toHaveTextContent('4 个活动终端');
  language.locale = 'en';
  rendered.rerender(
    <ConnectionOpenDialog view={{ ...view, count: 1, newBlocked: null }} choose={choose} />,
  );
  expect(screen.getByText(/Open terminal sessions for.*: 1/)).toBeVisible();
  expect(screen.getByRole('button', { name: 'New terminal' })).toBeEnabled();
});
it('aborts a request when its source unmounts', async () => {
  let signal: AbortSignal | undefined;
  const openConnection = vi.fn((_id: string, source?: AbortSignal) => {
    signal = source;
    return Promise.resolve(null);
  });
  function Source() {
    const open = useOpenConnection();
    return <button onClick={() => void open('id')}>Open</button>;
  }
  const rendered = render(
    <WorkspaceUiContext.Provider value={{ openConnection } as unknown as WorkspaceUiActions}>
      <Source />
    </WorkspaceUiContext.Provider>,
  );
  await userEvent.setup().click(screen.getByText('Open'));
  expect(signal?.aborted).toBe(false);
  rendered.unmount();
  expect(signal?.aborted).toBe(true);
});

it('focuses the reused terminal only after the choice closes', async () => {
  const focus = vi.fn();
  function Harness() {
    const [current, setCurrent] = useState<ConnectionOpenView | null>(view);
    return (
      <ConnectionOpenDialog
        view={current}
        choose={() => setCurrent(null)}
        focusReusedTerminal={focus}
      />
    );
  }
  render(<Harness />);
  await userEvent.setup().click(screen.getByRole('button', { name: '切换到已有终端' }));
  await waitFor(() => expect(focus).toHaveBeenCalledTimes(1));
});
