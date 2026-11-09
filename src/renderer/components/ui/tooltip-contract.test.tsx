// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { Button } from './button';
import { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider } from './tooltip';
afterEach(cleanup);
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi
    .fn()
    .mockReturnValue({ matches: false, addEventListener() {}, removeEventListener() {} });
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
it('default-open tooltip preserves descriptions and uses an explicit popup id', async () => {
  render(
    <TooltipProvider delay={0}>
      <span id="existing">Description</span>
      <Tooltip defaultOpen>
        <TooltipTrigger aria-describedby="existing" render={<Button>Target</Button>} />
        <TooltipContent id="tip">Helpful</TooltipContent>
      </Tooltip>
    </TooltipProvider>,
  );
  await screen.findByRole('tooltip');
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Target' })).toHaveAttribute(
      'aria-describedby',
      'existing tip',
    ),
  );
});
it('controlled tooltip closes without dropping an existing description', async () => {
  function Example() {
    const [open, setOpen] = useState(true);
    return (
      <TooltipProvider delay={0}>
        <button onClick={() => setOpen(false)}>Close tooltip</button>
        <Tooltip open={open} onOpenChange={setOpen}>
          <TooltipTrigger aria-describedby="existing" render={<Button>Target</Button>} />
          <TooltipContent>Helpful</TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }
  const user = userEvent.setup();
  render(<Example />);
  await screen.findByRole('tooltip');
  await user.click(screen.getByRole('button', { name: 'Close tooltip' }));
  await waitFor(() => expect(screen.queryByRole('tooltip')).not.toBeInTheDocument());
  expect(screen.getByRole('button', { name: 'Target' })).toHaveAttribute(
    'aria-describedby',
    'existing',
  );
});
it('focusable disabled buttons expose their explanation without activating', async () => {
  const action = vi.fn();
  const user = userEvent.setup();
  render(
    <TooltipProvider delay={0}>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button disabled focusableWhenDisabled onClick={action}>
              Unavailable
            </Button>
          }
        />
        <TooltipContent>Connect first</TooltipContent>
      </Tooltip>
    </TooltipProvider>,
  );
  const button = screen.getByRole('button', { name: 'Unavailable' });
  await user.tab();
  expect(button).toHaveFocus();
  await screen.findByRole('tooltip');
  await user.keyboard('{Enter} ');
  expect(action).not.toHaveBeenCalled();
  expect(button).toHaveAttribute('aria-disabled', 'true');
});
