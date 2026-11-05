// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { afterEach, expect, it } from 'vitest';
import { Meter } from './progress';
afterEach(cleanup);
it.each([
  [0, 'bg-status-success-foreground'],
  [69.99, 'bg-status-success-foreground'],
  [70, 'bg-status-warning'],
  [89.99, 'bg-status-warning'],
  [90, 'bg-destructive'],
  [100, 'bg-destructive'],
])('uses resource colors at %s percent while keeping the numeric value', (value, color) => {
  render(<Meter appearance="resource" aria-label="CPU" value={Number(value)} />);
  const meter = screen.getByRole('meter');
  expect(meter).toHaveAttribute('aria-valuenow', String(value));
  expect(meter.querySelector('[data-slot="meter-indicator"]')).toHaveClass(String(color));
});
it('updates color in both directions and leaves the default appearance unchanged', () => {
  const view = render(<Meter aria-label="disk" appearance="resource" value={95} />);
  view.rerender(<Meter aria-label="disk" appearance="resource" value={20} />);
  expect(screen.getByRole('meter').querySelector('[data-slot="meter-indicator"]')).toHaveClass(
    'bg-status-success-foreground',
  );
  view.rerender(<Meter aria-label="disk" value={95} />);
  expect(screen.getByRole('meter').querySelector('[data-slot="meter-indicator"]')).toHaveClass(
    'bg-primary',
  );
});
it('uses the configured range to determine resource percentage', () => {
  render(<Meter appearance="resource" aria-label="capacity" min={100} max={200} value={180} />);
  expect(screen.getByRole('meter').querySelector('[data-slot="meter-indicator"]')).toHaveClass(
    'bg-status-warning',
  );
});
