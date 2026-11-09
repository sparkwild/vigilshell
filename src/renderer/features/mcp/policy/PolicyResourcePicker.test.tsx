// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, afterEach, it, expect, vi } from 'vitest';
import type { ConnectionSummary } from '@contracts';
import { translate } from '../../../../shared/i18n/messages';
import { PolicyResourcePicker } from './PolicyResourcePicker';
const f = vi.hoisted(() => ({ read: vi.fn(), error: null as string | null }));
vi.mock('./use-policy-browser', () => ({
  usePolicyBrowser: () => ({ read: f.read, busy: false, error: f.error }),
}));
vi.mock('../../../app/DesktopProvider', () => ({
  useDesktop: () => ({
    t: (k: Parameters<typeof translate>[0], v?: Parameters<typeof translate>[2]) =>
      translate(k, 'en', v),
  }),
}));
const connection = { id: 'one', name: 'Fixture', revision: 1 } as ConnectionSummary;
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
  f.error = null;
  f.read.mockReset().mockImplementation(async (q) => ({
    kind: 'directory',
    path: q.path,
    page: {
      entries: [
        { path: '/srv', name: 'srv', kind: 'directory' },
        { path: '/file', name: 'file', kind: 'file' },
        { path: '/alias', name: 'alias', kind: 'symlink' },
      ],
      nextCursor: null,
    },
  }));
});
afterEach(cleanup);
it('keeps failed selections for retry and rejects unsupported object types', async () => {
  const user = userEvent.setup(),
    close = vi.fn(),
    apply = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  render(
    <PolicyResourcePicker connection={connection} selected={[]} close={close} apply={apply} />,
  );
  const file = await screen.findByRole('checkbox', { name: 'file' });
  expect(screen.getByRole('checkbox', { name: 'alias' })).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByRole('button', { name: 'Save protected paths' })).toBeDisabled();
  await user.click(file);
  await user.click(screen.getByRole('button', { name: 'Save protected paths' }));
  await screen.findByText('Save failed. Your selection is retained; retry to save');
  expect(close).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Save protected paths' }));
  await waitFor(() => expect(close).toHaveBeenCalledOnce());
  expect(apply).toHaveBeenLastCalledWith([{ path: '/file', kind: 'file' }]);
});
it('caps selection while allowing existing selections to be cleared', async () => {
  const selected = Array.from({ length: 128 }, (_, i) => ({
    path: i === 0 ? '/file' : `/item${i}`,
    kind: 'file' as const,
  }));
  const user = userEvent.setup();
  render(
    <PolicyResourcePicker
      connection={connection}
      selected={selected}
      close={() => {}}
      apply={vi.fn()}
    />,
  );
  expect(await screen.findByRole('checkbox', { name: 'srv' })).toHaveAttribute(
    'aria-disabled',
    'true',
  );
  await user.click(screen.getByRole('checkbox', { name: 'file' }));
  expect(screen.getByRole('checkbox', { name: 'srv' })).not.toHaveAttribute(
    'aria-disabled',
    'true',
  );
});
it('filters only loaded entries and preserves cross-directory selections', async () => {
  const user = userEvent.setup();
  render(
    <PolicyResourcePicker connection={connection} selected={[]} close={() => {}} apply={vi.fn()} />,
  );
  await user.click(await screen.findByRole('checkbox', { name: 'file' }));
  await user.type(screen.getByRole('textbox', { name: 'Search' }), 'srv');
  await user.click(within(screen.getByRole('treegrid')).getByRole('button', { name: 'srv' }));
  expect(screen.getByRole('textbox', { name: 'Search' })).toHaveValue('srv');
  await user.clear(screen.getByRole('textbox', { name: 'Search' }));
  expect(screen.getByRole('checkbox', { name: 'file' })).toBeChecked();
  expect(f.read).toHaveBeenLastCalledWith({ kind: 'directory', path: '/srv', after: null });
});

it('loads branches only when expanded, retains them on collapse, and marks parent coverage', async () => {
  f.read.mockImplementation(async (q) => ({
    kind: 'directory',
    path: q.path,
    page: {
      entries:
        q.path === '/'
          ? [{ path: '/srv', name: 'srv', kind: 'directory' }]
          : [{ path: '/srv/child', name: 'child', kind: 'file' }],
      nextCursor: null,
    },
  }));
  const user = userEvent.setup(),
    apply = vi.fn().mockResolvedValue(true);
  render(
    <PolicyResourcePicker connection={connection} selected={[]} close={() => {}} apply={apply} />,
  );
  await screen.findByRole('checkbox', { name: 'srv' });
  expect(f.read).toHaveBeenCalledTimes(1);
  await user.click(within(screen.getByRole('treegrid')).getByRole('button', { name: 'srv' }));
  await screen.findByRole('checkbox', { name: 'child' });
  await user.click(within(screen.getByRole('treegrid')).getByRole('button', { name: 'srv' }));
  expect(screen.queryByRole('checkbox', { name: 'child' })).not.toBeInTheDocument();
  await user.click(within(screen.getByRole('treegrid')).getByRole('button', { name: 'srv' }));
  expect(f.read).toHaveBeenCalledTimes(2);
  await user.click(screen.getByRole('checkbox', { name: 'srv' }));
  expect(screen.getByRole('checkbox', { name: 'child' })).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByRole('checkbox', { name: 'child' })).toBeChecked();
  await user.click(screen.getByRole('button', { name: 'Save protected paths' }));
  expect(apply).toHaveBeenCalledWith([{ path: '/srv', kind: 'directory' }]);
});
it('explains an unsupported item with the shared tooltip instead of inline text', async () => {
  const user = userEvent.setup();
  render(
    <PolicyResourcePicker connection={connection} selected={[]} close={() => {}} apply={vi.fn()} />,
  );
  const item = await screen.findByRole('checkbox', { name: 'alias' });
  expect(screen.queryByText('This type cannot be selected')).not.toBeInTheDocument();
  await user.hover(item.closest('[data-slot="tooltip-trigger"]')!);
  expect(await screen.findByRole('tooltip')).toHaveTextContent('This type cannot be selected');
});
it('offers per-branch retry and pagination without clearing selections', async () => {
  let failed = true;
  f.read.mockImplementation(async (q) => {
    if (q.path === '/srv' && failed) {
      failed = false;
      return null;
    }
    return {
      kind: 'directory',
      path: q.path,
      page:
        q.path === '/'
          ? { entries: [{ path: '/srv', name: 'srv', kind: 'directory' }], nextCursor: null }
          : q.after
            ? { entries: [{ path: '/srv/b', name: 'b', kind: 'file' }], nextCursor: null }
            : { entries: [{ path: '/srv/a', name: 'a', kind: 'file' }], nextCursor: 'a' },
    };
  });
  const user = userEvent.setup();
  render(
    <PolicyResourcePicker connection={connection} selected={[]} close={() => {}} apply={vi.fn()} />,
  );
  await user.click(await screen.findByRole('button', { name: 'srv' }));
  await user.click(await screen.findByRole('button', { name: 'Retry' }));
  await user.click(await screen.findByRole('checkbox', { name: 'a' }));
  await user.click(screen.getByRole('button', { name: translate('policy_more_entries', 'en') }));
  await screen.findByRole('checkbox', { name: 'b' });
  expect(screen.getByRole('checkbox', { name: 'a' })).toBeChecked();
});

it('expands ancestors and focuses a path without changing selection', async () => {
  f.read.mockImplementation(async (q) => ({
    kind: 'directory',
    path: q.path,
    page: {
      entries:
        q.path === '/'
          ? [{ path: '/srv', name: 'srv', kind: 'directory' }]
          : [{ path: '/srv/child', name: 'child', kind: 'file' }],
      nextCursor: null,
    },
  }));
  const user = userEvent.setup();
  render(
    <PolicyResourcePicker connection={connection} selected={[]} close={() => {}} apply={vi.fn()} />,
  );
  await screen.findByRole('checkbox', { name: 'srv' });
  await user.click(screen.getByRole('button', { name: 'Enter path' }));
  const address = screen.getByRole('textbox', { name: 'Remote path' });
  await user.clear(address);
  await user.type(address, '/srv/child');
  await user.keyboard('{Enter}');
  await waitFor(() =>
    expect(document.activeElement).toContainElement(
      screen.getByRole('checkbox', { name: 'child' }),
    ),
  );
  expect(screen.getByRole('checkbox', { name: 'child' })).not.toBeChecked();
});
