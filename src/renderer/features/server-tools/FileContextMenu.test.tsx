// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { RemoteEntry } from '@contracts';
import { ContextMenu, ContextMenuTrigger } from '../../components/ui/context-menu';
import { translate } from '../../../shared/i18n/messages';
import { FileContextMenu } from './FileContextMenu';
import type { FileMenuTarget } from './file-menu-target';
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: Parameters<typeof translate>[0]) => translate(key, 'en') }),
}));
afterEach(cleanup);
function fixture(target: FileMenuTarget) {
  const actions = {
    refresh: vi.fn(),
    open: vi.fn(),
    upload: vi.fn(),
    download: vi.fn(),
    archive: vi.fn(),
    create: vi.fn(),
    rename: vi.fn(),
    remove: vi.fn(),
    properties: vi.fn(),
    permissions: vi.fn(),
  };
  render(
    <ContextMenu>
      <ContextMenuTrigger>Target</ContextMenuTrigger>
      <FileContextMenu target={target} disabled={false} {...actions} />
    </ContextMenu>,
  );
  return actions;
}
const entry = { path: '/srv/a', name: 'a', kind: 'directory' } as RemoteEntry;
it('offers directory transfer and grouped creation without inheriting another listing path', async () => {
  const actions = fixture({
    source: 'tree',
    directory: '/srv/a',
    entries: [entry],
    runtimeId: 'runtime',
    groupId: 'group',
  });
  const user = userEvent.setup();
  await user.pointer({ target: screen.getByText('Target'), keys: '[MouseRight]' });
  expect(
    screen.getByRole('menuitem', { name: translate('tools_download', 'en') }),
  ).not.toHaveAttribute('aria-disabled', 'true');
  await user.click(screen.getByRole('menuitem', { name: 'Upload' }));
  expect(actions.upload).toHaveBeenCalledWith('/srv/a');
  await user.pointer({ target: screen.getByText('Target'), keys: '[MouseRight]' });
  screen.getByRole('menuitem', { name: 'New' }).focus();
  await user.keyboard('{ArrowRight}');
  (await screen.findByRole('menuitem', { name: 'File' })).focus();
  await user.keyboard('{Enter}');
  expect(actions.create).toHaveBeenCalledWith('/srv/a', 'file');
});
it('blank menus cannot mutate a stale selected row', async () => {
  const actions = fixture({
    source: 'blank',
    directory: '/srv',
    entries: [],
    runtimeId: 'runtime',
    groupId: 'group',
  });
  const user = userEvent.setup();
  await user.pointer({ target: screen.getByText('Target'), keys: '[MouseRight]' });
  expect(screen.queryByRole('menuitem', { name: 'Rename' })).not.toBeInTheDocument();
  expect(screen.queryByRole('menuitem', { name: 'Delete' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('menuitem', { name: 'Properties' }));
  expect(actions.properties).toHaveBeenCalledWith('/srv');
});
it('root menus keep metadata and download actions while disabling rename and deletion', async () => {
  fixture({ source: 'tree', directory: '/', entries: [], runtimeId: 'runtime', groupId: 'group' });
  const user = userEvent.setup();
  await user.pointer({ target: screen.getByText('Target'), keys: '[MouseRight]' });
  expect(screen.getByRole('menuitem', { name: 'Rename' })).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByRole('menuitem', { name: 'Delete' })).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByRole('menuitem', { name: 'File permissions' })).not.toHaveAttribute(
    'aria-disabled',
    'true',
  );
});
