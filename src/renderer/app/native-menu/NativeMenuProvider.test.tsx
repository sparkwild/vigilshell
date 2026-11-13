// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import type { NativeMenuCommand, NativeMenuState } from '@contracts';
import { Dialog, DialogContent, DialogTitle } from '../../components/ui/dialog';
import { NativeMenuProvider, useNativeMenuActions } from './NativeMenuProvider';

let receive: (command: NativeMenuCommand) => void;
const publish = vi.fn(),
  closeTab = vi.fn(),
  connect = vi.fn();
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof matchMedia;
  Object.defineProperty(window, 'vigilshell', {
    configurable: true,
    value: {
      onWorkspaceCommand: (listener: typeof receive) => {
        receive = listener;
        return () => {};
      },
      setNativeMenuState: publish,
    },
  });
  publish.mockClear();
  closeTab.mockClear();
  connect.mockClear();
});
afterEach(cleanup);
function Fixture() {
  const [open, setOpen] = useState(false);
  useNativeMenuActions({
    close: { enabled: true, closeTarget: 'tab', run: closeTab },
    connect: { enabled: true, run: connect },
  });
  return (
    <>
      <button onClick={() => setOpen(true)}>Open form</button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent closeLabel="Close form">
          <DialogTitle>Connection form</DialogTitle>
          <input aria-label="Draft" />
        </DialogContent>
      </Dialog>
    </>
  );
}
it('synchronizes native availability and closes the modal without closing a terminal behind it', async () => {
  render(
    <NativeMenuProvider>
      <Fixture />
    </NativeMenuProvider>,
  );
  await waitFor(() =>
    expect(publish).toHaveBeenLastCalledWith({
      enabled: ['close', 'connect'],
      checked: [],
      closeTarget: 'tab',
    }),
  );
  await userEvent.click(screen.getByText('Open form'));
  await waitFor(() =>
    expect(publish.mock.lastCall![0] as NativeMenuState).toEqual({
      enabled: ['close'],
      checked: [],
      closeTarget: 'dialog',
    }),
  );
  act(() => receive('connect'));
  expect(connect).not.toHaveBeenCalled();
  act(() => receive('close'));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(closeTab).not.toHaveBeenCalled();
  await waitFor(() =>
    expect((publish.mock.lastCall![0] as NativeMenuState).closeTarget).toBe('tab'),
  );
  await act(async () => receive('close'));
  expect(closeTab).toHaveBeenCalledOnce();
});
