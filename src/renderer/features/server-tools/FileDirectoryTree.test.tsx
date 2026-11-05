// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import type { TerminalSessionView } from '@contracts';
import { FileDirectoryTree } from './FileDirectoryTree';
const fixture = vi.hoisted(() => ({
  navigate: vi.fn(),
  service: {
    preference: () => ({ expanded: new Set(['/']) }),
    read: vi.fn(async () => ({
      entries: [
        { path: '/data', name: 'data', kind: 'directory' },
        { path: '/etc', name: 'etc', kind: 'directory' },
      ],
    })),
    persist: vi.fn(),
  },
  store: {},
}));
vi.mock('./directory-service', () => ({ directories: () => fixture.service }));
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({ store: fixture.store }),
}));
vi.mock('../../app/DesktopProvider', () => ({ useDesktop: () => ({ t: (key: string) => key }) }));
// Retain actual Tree selection behavior; jsdom has no virtualized viewport geometry.
vi.mock('react-aria-components', async () => ({
  ...(await vi.importActual<typeof import('react-aria-components')>('react-aria-components')),
  Virtualizer: ({ children }: { children: React.ReactNode }) => children,
}));
afterEach(cleanup);
it('moves the global selected state on successive single clicks and external navigation', async () => {
  const session = { connectionId: 'fixture' } as TerminalSessionView;
  function Browser() {
    const [path, setPath] = useState('/');
    return (
      <>
        <button onClick={() => setPath('/')}>Go to root</button>
        <FileDirectoryTree
          session={session}
          online
          currentPath={path}
          navigate={(next) => {
            fixture.navigate(next);
            setPath(next);
          }}
        />
      </>
    );
  }
  const user = userEvent.setup();
  render(<Browser />);
  const row = async (name: string) => (await screen.findByText(name)).closest('[role="row"]')!;
  const data = await row('data'),
    etc = await row('etc');
  await user.click(within(data as HTMLElement).getByText('data'));
  expect(data).toHaveAttribute('data-selected', 'true');
  await user.click(within(etc as HTMLElement).getByText('etc'));
  expect(etc).toHaveAttribute('data-selected', 'true');
  expect(data).not.toHaveAttribute('data-selected');
  expect(fixture.navigate.mock.calls).toEqual([['/data'], ['/etc']]);
  await user.click(screen.getByRole('button', { name: 'Go to root' }));
  expect(await row('/')).toHaveAttribute('data-selected', 'true');
  expect(etc).not.toHaveAttribute('data-selected');
});

it('right click reports the target without navigating the open directory', async () => {
  const navigate = vi.fn(),
    context = vi.fn();
  render(
    <FileDirectoryTree
      session={{ connectionId: 'fixture' } as TerminalSessionView}
      online
      currentPath="/"
      navigate={navigate}
      onContextMenu={context}
    />,
  );
  const user = userEvent.setup();
  await user.pointer({ target: await screen.findByText('data'), keys: '[MouseRight]' });
  expect(context).toHaveBeenCalledWith(expect.objectContaining({ path: '/data' }));
  expect(navigate).not.toHaveBeenCalled();
});
