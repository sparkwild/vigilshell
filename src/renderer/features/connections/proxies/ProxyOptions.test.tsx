// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { translate } from '../../../../shared/i18n/messages';
import { ProxyOptions } from './ProxyOptions';
const f = vi.hoisted(() => ({
  call: vi.fn(),
  mutation: vi.fn(),
  confirm: vi.fn(),
  feedback: { forOwner: () => undefined, isBusy: () => false, setAnchor: () => {} },
}));
vi.mock('../../../app/DesktopProvider', () => {
  const t = (key: Parameters<typeof translate>[0]) => translate(key, 'en');
  return { useDesktop: () => ({ t }) };
});
vi.mock('../../../shared/ConfirmationProvider', () => ({ useConfirmation: () => f.confirm }));
vi.mock('../ConnectionProvider', () => ({
  useConnections: () => ({ operations: f.feedback, snapshot: { appearance: { locale: 'en' } } }),
}));
vi.mock('../workspace-context', async (original) => ({
  ...(await original<object>()),
  useWorkspace: () => ({ state: { version: 1, runtimeId: 'runtime', ready: true }, store: f }),
}));
beforeEach(() => {
  Element.prototype.getAnimations = () => [];
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
it('lists referencing names and does not delete a referenced proxy or prompt for confirmation', async () => {
  f.call.mockImplementation(async (name: string) =>
    name === 'listProxies'
      ? {
          kind: 'proxies',
          version: 1,
          next: null,
          proxies: [
            {
              id: 'proxy',
              version: 1,
              credentialSaved: false,
              config: {
                name: 'Shared proxy',
                kind: 'http',
                host: 'localhost',
                port: 8080,
                username: '',
              },
            },
          ],
        }
      : {
          kind: 'proxyReferences',
          version: 1,
          next: null,
          connectionNames: ['Existing connection'],
        },
  );
  render(
    <ProxyOptions
      value="proxy"
      onChange={vi.fn()}
      disabled={false}
      credentialBlocked={false}
      onChildClose={vi.fn()}
    />,
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Delete proxy · Shared proxy' }));
  expect(await screen.findByText(/Existing connection/)).toBeInTheDocument();
  expect(f.confirm).not.toHaveBeenCalled();
  expect(f.mutation).not.toHaveBeenCalled();
});

it('loads and selects a saved proxy beyond the first page with the directory version', async () => {
  const proxies = Array.from({ length: 75 }, (_, index) => ({
    id: `proxy-${index}`,
    version: 3,
    credentialSaved: index === 74,
    config: {
      name: `Proxy ${index}`,
      kind: 'http',
      host: 'localhost',
      port: 8080,
      username: '',
    },
  }));
  f.call.mockImplementation(async (name: string, params: { offset: number }) => {
    expect(name).toBe('listProxies');
    return {
      kind: 'proxies',
      proxies: proxies.slice(params.offset, params.offset + 50),
      version: 9,
      next: params.offset === 0 ? 50 : null,
    };
  });
  render(
    <ProxyOptions
      value="proxy-74"
      onChange={vi.fn()}
      disabled={false}
      credentialBlocked={false}
      onChildClose={vi.fn()}
    />,
  );
  expect(await screen.findByRole('button', { name: 'Edit proxy · Proxy 74' })).toBeInTheDocument();
  expect(screen.getByRole('combobox')).toHaveTextContent('Proxy 74');
  expect(f.call.mock.calls).toEqual([
    ['listProxies', { offset: 0, version: null }],
    ['listProxies', { offset: 50, version: 9 }],
  ]);
  await userEvent.click(screen.getByRole('button', { name: 'Edit proxy · Proxy 74' }));
  expect(await screen.findByText(translate('ws_credential_saved', 'en'))).toBeInTheDocument();
  expect(screen.getByRole('textbox', { name: 'Name' })).toHaveValue('Proxy 74');
  expect(f.mutation).not.toHaveBeenCalled();
});
