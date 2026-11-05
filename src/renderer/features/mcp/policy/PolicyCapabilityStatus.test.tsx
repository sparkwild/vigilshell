// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { render, screen, cleanup, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { PolicyCapabilityStatus } from './PolicyCapabilityStatus';
import { defaultPolicy } from '../model';
import { translate } from '../../../../shared/i18n/messages';
vi.mock('../../../app/DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: Parameters<typeof translate>[0]) => translate(key, 'en') }),
}));
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it('does not query until expanded and distinguishes configured permission from unsupported execution', async () => {
  Element.prototype.getAnimations = () => [];
  const request = vi.fn().mockResolvedValue({
    ok: true,
    reply: {
      kind: 'policyCapabilities',
      environment_known: false,
      tools: [
        {
          tool: 'packages_manage.install',
          configured: true,
          supported: false,
          usable: false,
          reason: 'session_not_ready',
          requiresExecutionCheck: true,
        },
      ],
    },
  });
  vi.stubGlobal('vigilshell', { mcp: { request } });
  const policy = defaultPolicy('connection');
  const { rerender } = render(
    <PolicyCapabilityStatus policy={policy} valid environmentKey="unknown" />,
  );
  expect(request).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Review effective capabilities' }));
  const row = await screen.findByRole('row', { name: /Install software/ });
  expect(within(row).getAllByText('Not checked')).toHaveLength(2);
  expect(within(row).getByText('Allowed')).toBeInTheDocument();
  request.mockResolvedValue({
    ok: true,
    reply: {
      kind: 'policyCapabilities',
      environment_known: true,
      tools: [
        {
          tool: 'packages_manage.install',
          configured: true,
          supported: false,
          usable: false,
          reason: 'operation_unsupported',
          requiresExecutionCheck: true,
        },
      ],
    },
  });
  rerender(<PolicyCapabilityStatus policy={policy} valid environmentKey="ready" />);
  await waitFor(() => expect(screen.getAllByText('Unsupported')).toHaveLength(2));
  expect(request.mock.calls.every(([r]) => r.action === 'previewPolicy')).toBe(true);
  rerender(<PolicyCapabilityStatus policy={policy} valid={false} environmentKey="ready" />);
  expect(screen.getByText('Complete or correct the draft settings first')).toBeInTheDocument();
  expect(screen.queryByRole('grid')).not.toBeInTheDocument();
});
