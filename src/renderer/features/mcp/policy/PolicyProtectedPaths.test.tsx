// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { useState } from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ProtectedPath } from '@contracts';
import { translate } from '../../../../shared/i18n/messages';
import { PolicyProtectedPaths } from './PolicyProtectedPaths';
import { normalizeProtectedPath, validProtectedPath } from './protected-path';
vi.mock('../../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (key: Parameters<typeof translate>[0], values?: Parameters<typeof translate>[2]) =>
      translate(key, 'en', values),
  }),
}));
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi
    .fn()
    .mockReturnValue({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() });
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
afterEach(cleanup);
function Harness({ initial = [{ path: '', kind: 'directory' }] as ProtectedPath[] }) {
  const [paths, setPaths] = useState(initial);
  return <PolicyProtectedPaths paths={paths} busy={false} change={setPaths} />;
}
it('starts without an error and updates format validation when switching type', async () => {
  const user = userEvent.setup();
  render(<Harness />);
  expect(screen.getByRole('textbox', { name: 'Directory path' })).toHaveAttribute(
    'aria-invalid',
    'false',
  );
  expect(screen.queryByText('Remote path 1')).not.toBeInTheDocument();
  await user.type(screen.getByRole('textbox'), '/srv/data/');
  await user.tab();
  expect(screen.getByRole('textbox')).toHaveValue('/srv/data');
  await user.click(screen.getByRole('combobox', { name: 'Path type' }));
  await user.click(screen.getByRole('option', { name: 'File' }));
  const input = screen.getByRole('textbox', { name: 'File path' });
  await user.clear(input);
  await user.type(input, '/srv/data/');
  await user.tab();
  expect(input).toHaveAttribute('aria-invalid', 'true');
  await user.clear(input);
  await user.type(input, '/etc/hosts');
  await user.tab();
  expect(input).toHaveAttribute('aria-invalid', 'false');
});
it('flags canonical duplicates after directory input normalization', async () => {
  const user = userEvent.setup();
  render(
    <Harness
      initial={[
        { path: '/srv', kind: 'directory' },
        { path: '', kind: 'directory' },
      ]}
    />,
  );
  await user.type(screen.getAllByRole('textbox')[1]!, '/srv/');
  await user.tab();
  expect(screen.getAllByText('This path is already added')).toHaveLength(2);
});
it.each([
  ['directory', '/', true],
  ['file', '/', false],
  ['file', '/etc/hosts', true],
  ['file', '/home/.profile', true],
  ['directory', '/srv/data.v1', true],
  ['file', '/etc/', false],
  ['directory', 'relative', false],
  ['file', '/a/../b', false],
  ['directory', '/a//b', false],
  ['file', '/a\nb', false],
] as const)('validates %s %s as %s', (kind, path, valid) =>
  expect(validProtectedPath({ kind, path })).toBe(valid),
);
it('normalizes only a single directory suffix, retaining malformed paths for feedback', () => {
  expect(normalizeProtectedPath({ kind: 'directory', path: '/srv/' }).path).toBe('/srv');
  expect(normalizeProtectedPath({ kind: 'directory', path: '/srv//' }).path).toBe('/srv//');
  expect(normalizeProtectedPath({ kind: 'file', path: '/srv/' }).path).toBe('/srv/');
});
