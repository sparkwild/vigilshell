// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { CredentialStorageNotice } from './CredentialStorageNotice';
import { TooltipProvider } from '../../components/ui/tooltip';
import { translate, type MessageKey } from '../../../shared/i18n/messages';

vi.mock('../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: MessageKey) => translate(key, 'zh-CN') }),
}));
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
afterEach(cleanup);

it('shows both descriptions in the global tooltip on hover', async () => {
  render(
    <TooltipProvider delay={0}>
      <CredentialStorageNotice />
    </TooltipProvider>,
  );
  const user = userEvent.setup();
  const trigger = screen.getByRole('button', { name: '钥匙串授权说明' });
  expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  await user.hover(trigger);
  expect(await screen.findByRole('tooltip')).toBeVisible();
  expect(screen.getByText(translate('ws_keychain_help', 'zh-CN'))).toBeVisible();
  expect(screen.getByText(translate('ws_credential_storage_notice', 'zh-CN'))).toBeVisible();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  await user.unhover(trigger);
  await waitFor(() => expect(screen.queryByRole('tooltip')).not.toBeInTheDocument());
});

it('provides the combined description on keyboard focus without submitting the form', async () => {
  const submit = vi.fn((event: React.FormEvent) => event.preventDefault());
  render(
    <TooltipProvider delay={0}>
      <form onSubmit={submit}>
        <CredentialStorageNotice />
      </form>
    </TooltipProvider>,
  );
  const user = userEvent.setup();
  const trigger = screen.getByRole('button', { name: '钥匙串授权说明' });
  await user.tab();
  expect(await screen.findByRole('tooltip')).toBeVisible();
  expect(trigger).toHaveAccessibleDescription(/保存的密码和私钥/);
  expect(trigger).toHaveAccessibleDescription(/不是服务器密码/);
  await user.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('tooltip')).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
  await user.click(trigger);
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(submit).not.toHaveBeenCalled();
});
