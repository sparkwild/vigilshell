// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { IconButton } from './icon-button';
beforeEach(() => {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  Element.prototype.getAnimations = () => [];
});
afterEach(cleanup);
it('preserves the label and disabled action while tooltip listeners survive re-enabling', async () => {
  const click = vi.fn(),
    user = userEvent.setup();
  const element = (disabled: boolean) => (
    <IconButton label="Refresh" tooltip="Reload directory" disabled={disabled} onClick={click}>
      ↻
    </IconButton>
  );
  const view = render(element(true));
  expect(screen.getByRole('button', { name: 'Refresh' })).toBeDisabled();
  await user.tab();
  await screen.findByRole('tooltip', { name: 'Reload directory' });
  await user.keyboard('{Enter}');
  expect(click).not.toHaveBeenCalled();
  view.rerender(element(false));
  await user.tab();
  const button = screen.getByRole('button', { name: 'Refresh' });
  await user.hover(button);
  await screen.findByRole('tooltip', { name: 'Reload directory' });
  await user.click(button);
  expect(click).toHaveBeenCalledTimes(1);
});
