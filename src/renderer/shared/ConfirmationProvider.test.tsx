// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
afterEach(cleanup);
beforeEach(() => {
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
import { ConfirmationProvider, useConfirmation } from './ConfirmationProvider';

it('serializes requests, focuses cancel, and cancels pending requests on unmount', async () => {
  const results = vi.fn();
  function Caller() {
    const confirm = useConfirmation();
    return (
      <button
        onClick={() => {
          for (const title of ['First', 'Second']) {
            void confirm({
              title,
              description: 'Remove this item?',
              cancelLabel: 'Cancel',
              confirmLabel: 'Remove',
            }).then((value) => results(title, value));
          }
        }}
      >
        Request
      </button>
    );
  }
  const user = userEvent.setup();
  const view = render(
    <ConfirmationProvider>
      <Caller />
    </ConfirmationProvider>,
  );
  await user.click(screen.getByText('Request'));
  const dialog = screen.getByRole('alertdialog', { name: 'First' });
  await waitFor(() => expect(within(dialog).getByText('Cancel')).toHaveFocus());
  await user.click(within(dialog).getByText('Remove'));
  await waitFor(() => expect(results).toHaveBeenCalledWith('First', true));
  expect(screen.getByRole('alertdialog', { name: 'Second' })).toBeVisible();
  await waitFor(() => expect(screen.getByText('Cancel')).toHaveFocus());
  view.unmount();
  await waitFor(() => expect(results).toHaveBeenCalledWith('Second', false));
});

it('keeps failed actions open for retry and blocks cancellation and duplicate execution while busy', async () => {
  let complete!: (accepted: boolean) => void;
  const execute = vi.fn(
    () =>
      new Promise<boolean>((resolve) => {
        complete = resolve;
      }),
  );
  const result = vi.fn();
  function Caller() {
    const confirm = useConfirmation();
    return (
      <button
        onClick={() =>
          void confirm({
            title: 'Delete connection?',
            description: 'Remove this connection.',
            detail: <span>2 active sessions</span>,
            tone: 'danger',
            confirmLabel: 'Delete',
            cancelLabel: 'Keep',
            onConfirm: execute,
          }).then(result)
        }
      >
        Request
      </button>
    );
  }
  const user = userEvent.setup();
  render(
    <ConfirmationProvider>
      <Caller />
    </ConfirmationProvider>,
  );
  await user.click(screen.getByText('Request'));
  expect(screen.getByText('2 active sessions')).toBeVisible();
  await user.click(screen.getByText('Delete'));
  await user.keyboard('{Escape}');
  expect(screen.getByRole('alertdialog')).toBeVisible();
  expect(screen.getByText('Keep')).toBeDisabled();
  await user.click(screen.getByText('Delete'));
  expect(execute).toHaveBeenCalledTimes(1);
  await act(async () => complete(false));
  expect(result).not.toHaveBeenCalled();
  expect(screen.getByText('Delete')).toBeEnabled();
  await user.click(screen.getByText('Delete'));
  await act(async () => complete(true));
  expect(result).toHaveBeenCalledWith(true);
  await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument());
});

it('cancels requests belonging to an unmounted caller without losing another caller request', async () => {
  const result = vi.fn();
  function Caller({ name }: { name: string }) {
    const confirm = useConfirmation();
    return (
      <button
        onClick={() =>
          void confirm({
            title: name,
            description: name,
            confirmLabel: 'Accept',
            cancelLabel: 'Cancel',
          }).then((value) => result(name, value))
        }
      >
        {name}
      </button>
    );
  }
  const user = userEvent.setup();
  const view = render(
    <ConfirmationProvider>
      <Caller name="First" />
      <Caller name="Second" />
    </ConfirmationProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'First' }));
  view.rerender(
    <ConfirmationProvider>
      <Caller name="Second" key="second" />
    </ConfirmationProvider>,
  );
  await waitFor(() => expect(result).toHaveBeenCalledWith('First', false));
  await user.click(screen.getByRole('button', { name: 'Second' }));
  await user.click(screen.getByText('Accept'));
  expect(result).toHaveBeenCalledWith('Second', true);
});

it('rejects thrown action errors and releases the dialog', async () => {
  const error = new Error('operation failed');
  const rejected = vi.fn();
  function Caller() {
    const confirm = useConfirmation();
    return (
      <button
        onClick={() =>
          void confirm({
            title: 'Delete?',
            description: 'Description',
            confirmLabel: 'Delete',
            cancelLabel: 'Cancel',
            onConfirm: async () => {
              throw error;
            },
          }).catch(rejected)
        }
      >
        Request
      </button>
    );
  }
  const user = userEvent.setup();
  render(
    <ConfirmationProvider>
      <Caller />
    </ConfirmationProvider>,
  );
  await user.click(screen.getByText('Request'));
  await user.click(screen.getByText('Delete'));
  await waitFor(() => expect(rejected).toHaveBeenCalledWith(error));
  await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument());
});
