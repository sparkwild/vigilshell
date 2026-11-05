// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import {
  NavigationInteractionProvider,
  useNavigationInteraction,
  useNavigationInteractionHeld,
} from './navigation-interaction';

afterEach(cleanup);

function Holder({ held }: { held: boolean }) {
  useNavigationInteraction(held);
  return null;
}

function Status() {
  return <output>{String(useNavigationInteractionHeld())}</output>;
}

it('retains the preview until every surface releases, including unmount cleanup', () => {
  function Surfaces({ first, second }: { first: boolean; second?: boolean }) {
    return (
      <NavigationInteractionProvider>
        <Holder held={first} />
        {second !== undefined ? <Holder held={second} /> : null}
        <Status />
      </NavigationInteractionProvider>
    );
  }
  const view = render(<Surfaces first={false} second={false} />);
  expect(screen.getByRole('status').textContent).toBe('false');
  view.rerender(<Surfaces first second />);
  expect(screen.getByRole('status').textContent).toBe('true');
  view.rerender(<Surfaces first={false} second />);
  expect(screen.getByRole('status').textContent).toBe('true');
  view.rerender(<Surfaces first={false} />);
  expect(screen.getByRole('status').textContent).toBe('false');
  view.rerender(<Surfaces first second={false} />);
  expect(screen.getByRole('status').textContent).toBe('true');
  view.rerender(<Surfaces first={false} second={false} />);
  expect(screen.getByRole('status').textContent).toBe('false');
});
