// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { RemoteEntry } from '@contracts';
import { translate } from '../../../shared/i18n/messages';
import { FilePermissionsDialog } from './FilePermissionsDialog';
vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: Parameters<typeof translate>[0]) => translate(key, 'en') }),
}));
vi.mock('../connections/workspace-context', () => ({ errorKey: () => 'tools_unavailable' }));
afterEach(cleanup);
it('validates octal input and submits explicit mode without enabling recursive changes', async () => {
  const apply = vi.fn().mockResolvedValue(false);
  render(
    <FilePermissionsDialog
      entries={[{ path: '/srv', kind: 'directory', permissions: 0o4755 } as RemoteEntry]}
      error={null}
      close={vi.fn()}
      apply={apply}
    />,
  );
  const user = userEvent.setup(),
    input = screen.getByRole('textbox', { name: 'Octal permissions' });
  await user.clear(input);
  await user.type(input, '888');
  expect(screen.getByRole('button', { name: 'Confirm' })).toBeDisabled();
  await user.clear(input);
  await user.type(input, '640');
  await user.click(screen.getByRole('button', { name: 'Confirm' }));
  expect(apply).toHaveBeenCalledWith({ recursive: false, setMask: 0o640, clearMask: 0o137 });
  expect(input).toHaveValue('640');
});
it('keeps multiple selection unchanged initially and never enables recursive changes on root', () => {
  render(
    <FilePermissionsDialog
      entries={[
        { path: '/', kind: 'directory', permissions: 0o755 } as RemoteEntry,
        { path: '/file', kind: 'file', permissions: 0o600 } as RemoteEntry,
      ]}
      error={null}
      close={vi.fn()}
      apply={vi.fn()}
    />,
  );
  expect(screen.getByRole('button', { name: 'Confirm' })).toBeDisabled();
  expect(
    screen.getByRole('checkbox', { name: 'Apply to files and subdirectories' }),
  ).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getAllByRole('combobox')).toHaveLength(9);
});
