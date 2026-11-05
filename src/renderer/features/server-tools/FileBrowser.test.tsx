// @vitest-environment jsdom
import { render, screen, waitFor, cleanup, fireEvent, act, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, it, expect, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { FileBrowser } from './file-browser';
import { translate } from '../../../shared/i18n/messages';
import type { AgentTask, TerminalSessionView } from '@contracts';
const fixture = vi.hoisted(() => ({
  call: vi.fn(),
  confirm: vi.fn(),
  resize: vi.fn(),
  treeWidth: 180,
  layoutChanged: undefined as
    undefined | ((layout: object, meta: { isUserInteraction: boolean }) => void),
  store: {} as Record<string, unknown>,
}));
// jsdom has zero geometry; omit pointer-resize hit testing for keyboard behavior tests.
vi.mock('../../components/ui/resizable', async () => {
  const { useLayoutEffect } = await import('react');
  return {
    ResizablePanelGroup: ({
      children,
      onLayoutChanged,
    }: {
      children: React.ReactNode;
      onLayoutChanged?: typeof fixture.layoutChanged;
    }) => {
      fixture.layoutChanged = onLayoutChanged;
      return <div>{children}</div>;
    },
    ResizablePanel: ({
      children,
      panelRef,
      defaultSize,
    }: {
      children: React.ReactNode;
      panelRef?: { current: unknown };
      defaultSize?: string;
    }) => {
      useLayoutEffect(() => {
        if (!panelRef) return;
        panelRef.current = {
          resize: fixture.resize,
          getSize: () => ({ inPixels: fixture.treeWidth }),
        };
        return () => {
          panelRef.current = null;
        };
      }, [panelRef]);
      return (
        <div
          data-testid={panelRef ? 'directory-panel' : undefined}
          data-default-width={defaultSize}
        >
          {children}
        </div>
      );
    },
    ResizableHandle: () => null,
  };
});
vi.mock('react-aria-components', async () => {
  const actual =
    await vi.importActual<typeof import('react-aria-components')>('react-aria-components');
  return {
    ...actual,
    Virtualizer: ({ children }: { children: React.ReactNode }) => children,
  };
});
vi.mock('../connections/workspace-context', () => ({
  useWorkspace: () => ({
    store: fixture.store,
    state: { runtimeId: 'r', transfers: new Map(), operations: new Map() },
  }),
  errorCode: () => 'tools_unavailable',
  errorKey: () => 'ws_error_tools_unavailable',
}));
vi.mock('../connections/ui-context', () => ({
  useWorkspaceUi: () => ({ setToolDialogClose: vi.fn() }),
}));
vi.mock('../../shared/ConfirmationProvider', () => ({
  useConfirmation: () => fixture.confirm,
}));
vi.mock('../../app/DesktopProvider', async () => {
  const React = await import('react');
  const { defaultPreferences } = await import('../../../shared/preferences');
  return {
    useDesktop: () => {
      const [preferences, setPreferences] = React.useState(defaultPreferences);
      return {
        t: (key: Parameters<typeof translate>[0]) => translate(key, 'zh-CN'),
        snapshot: {
          preferences,
          runtime: { status: 'ready' },
          save: { phase: 'idle' },
        },
        store: {
          updateFields: async (patch: Partial<typeof preferences>) => {
            setPreferences((old) => ({ ...old, ...patch }));
            return { error: null };
          },
        },
      };
    },
  };
});
vi.mock('../../shared/platform/ui-state-store', () => ({
  useUiStateErrors: () => [],
  loadUiState: async () => {},
  readUiState: () => ({ data: null }),
  updateUiState: async () => {},
}));

const session = {
  id: 'session',
  connectionId: 'connection',
  groupId: 'group',
  phase: 'running',
} as TerminalSessionView;
beforeEach(() => {
  fixture.resize.mockReset();
  fixture.treeWidth = 180;
  fixture.layoutChanged = undefined;
  fixture.confirm.mockResolvedValue(true);
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })) as unknown as typeof window.matchMedia;
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  const entry = (name: string, size: number) => ({
    name,
    path: `/root/${name}`,
    kind: 'file',
    size,
    modified: null,
    permissions: 0o644,
    uid: 0,
    gid: 0,
  });
  fixture.call.mockImplementation(async (method: string, p: { path: string; cursor?: string }) =>
    method === 'listFiles'
      ? {
          kind: 'directory',
          page: {
            path: p.path === '.' ? '/root' : p.path,
            entries:
              p.path === '/'
                ? []
                : [entry(p.cursor ? 'second.txt' : 'tiny.txt', p.cursor ? 1024 : 18)],
            next: p.path === '.' && !p.cursor ? '1' : null,
          },
        }
      : method === 'resolveFileIdentities'
        ? { kind: 'identities', names: [] }
        : { kind: 'done' },
  );
  fixture.store = {
    call: fixture.call,
    mutation: fixture.call,
    getSnapshot: () => ({ runtimeId: 'r', sessions: [session] }),
    report: vi.fn(),
  };
});

it('keeps a changed file name on outside click and honours discard confirmation on Escape', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '新建目录' }));
  const input = screen.getByRole('textbox', { name: '名称' });
  await user.type(input, 'reports');
  await user.click(document.querySelector('[data-slot="dialog-overlay"]')!);
  expect(input).toHaveValue('reports');
  expect(fixture.confirm).not.toHaveBeenCalled();
  fixture.confirm.mockResolvedValue(false);
  await user.keyboard('{Escape}');
  await waitFor(() => expect(fixture.confirm).toHaveBeenCalledTimes(1));
  expect(input).toHaveValue('reports');
  fixture.confirm.mockResolvedValue(true);
  await user.click(screen.getByRole('button', { name: '取消' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});

it('preserves failed drafts, submits with Enter, and prevents duplicate or IME submission', async () => {
  let fail = true,
    finish: (() => void) | undefined;
  const mutation = vi.fn(async () => {
    if (fail) throw new Error('permission denied');
    await new Promise<void>((resolve) => {
      finish = resolve;
    });
  });
  fixture.store.mutation = mutation;
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '新建目录' }));
  const input = screen.getByRole('textbox', { name: '名称' });
  await user.type(input, 'reports');
  expect(fireEvent.keyDown(input, { key: 'Enter', isComposing: true })).toBe(false);
  expect(mutation).not.toHaveBeenCalled();
  await user.keyboard('{Enter}');
  await waitFor(() => expect(mutation).toHaveBeenCalledTimes(1));
  expect(input).toHaveValue('reports');
  await waitFor(() => expect(input).toBeEnabled());
  expect(input).toHaveAttribute('aria-invalid', 'true');
  expect(input).toHaveAccessibleDescription();
  fail = false;
  await user.keyboard('{Enter}');
  await waitFor(() => expect(mutation).toHaveBeenCalledTimes(2));
  expect(screen.getByRole('button', { name: '确认' })).toBeDisabled();
  await user.keyboard('{Enter}');
  expect(mutation).toHaveBeenCalledTimes(2);
  finish?.();
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
it('renders all pages as single rows and supports filtering and keyboard multi-selection', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  expect(screen.getByText('second.txt')).toBeInTheDocument();
  expect(screen.getByText('18 B')).toBeInTheDocument();
  for (const name of ['类型', '权限', '用户／用户组'])
    expect(screen.getByRole('columnheader', { name: new RegExp(name) })).toBeInTheDocument();
  expect(screen.getAllByText('-rw-r--r--')).toHaveLength(2);
  expect(screen.getAllByText('0/0')).toHaveLength(2);
  const user = userEvent.setup();
  await user.click(screen.getByText('tiny.txt'));
  await user.keyboard('{Control>}a{/Control}');
  await waitFor(() => expect(screen.getByText('2 · 2 已选')).toBeInTheDocument());
  await user.type(screen.getByRole('textbox', { name: '筛选当前目录' }), 'second');
  expect(screen.queryByText('tiny.txt')).not.toBeInTheDocument();
  expect(screen.getByText('second.txt')).toBeInTheDocument();
});

it('renders an empty directory with the shared Empty component and retains the table header', async () => {
  const original = fixture.call.getMockImplementation()!;
  fixture.call.mockImplementation((method: string, params: { path: string }) =>
    method === 'listFiles' && params.path === '.'
      ? Promise.resolve({ kind: 'directory', page: { path: '/root', entries: [], next: null } })
      : original(method, params),
  );
  render(<FileBrowser session={session} online visible />);
  const title = await screen.findByText('此目录为空');
  expect(title.closest('[data-slot="empty"]')).toBeInTheDocument();
  expect(title.closest('.file-table-state')).toHaveAttribute('role', 'status');
  expect(screen.getByRole('columnheader', { name: /名称/ })).toBeInTheDocument();
});

it('distinguishes a filtered result from an empty directory and can clear the filter', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  await user.type(screen.getByRole('textbox', { name: '筛选当前目录' }), 'no-match');
  expect(screen.getByText('没有匹配的文件').closest('[data-slot="empty"]')).toBeInTheDocument();
  expect(screen.queryByText('此目录为空')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '清除筛选' }));
  expect(screen.getByText('tiny.txt')).toBeInTheDocument();
  expect(screen.getByText('second.txt')).toBeInTheDocument();
});

it('explains hidden-only directories and offers to show their entries', async () => {
  const original = fixture.call.getMockImplementation()!;
  fixture.call.mockImplementation((method: string, params: { path: string }) =>
    method === 'listFiles' && params.path === '.'
      ? Promise.resolve({
          kind: 'directory',
          page: {
            path: '/root',
            entries: [{ name: '.env', path: '/root/.env', kind: 'file', size: 0 }],
            next: null,
          },
        })
      : original(method, params),
  );
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('.env');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '更多操作' }));
  await user.click(await screen.findByRole('menuitemcheckbox', { name: '显示隐藏文件' }));
  await user.keyboard('{Escape}');
  expect(screen.getByText('没有可显示的文件').closest('[data-slot="empty"]')).toBeInTheDocument();
  expect(screen.queryByText('此目录为空')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '显示隐藏文件' }));
  expect(screen.getByText('.env')).toBeInTheDocument();
});

it('keeps a pending directory read in the loading state until it resolves', async () => {
  const original = fixture.call.getMockImplementation()!;
  let finish!: (reply: unknown) => void;
  fixture.call.mockImplementation((method: string, params: { path: string }) =>
    method === 'listFiles' && params.path === '.'
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : original(method, params),
  );
  render(<FileBrowser session={session} online visible />);
  await waitFor(() => expect(finish).toBeTypeOf('function'));
  expect(document.querySelector('.file-table-state [data-slot="spinner"]')).toBeInTheDocument();
  expect(screen.queryByText('此目录为空')).not.toBeInTheDocument();
  await act(async () => {
    finish({ kind: 'directory', page: { path: '/root', entries: [], next: null } });
  });
  expect(await screen.findByText('此目录为空')).toBeInTheDocument();
});

it('shows read failures as an error rather than reporting an empty directory', async () => {
  const original = fixture.call.getMockImplementation()!;
  fixture.call.mockImplementation((method: string, params: { path: string }) =>
    method === 'listFiles' && params.path === '.'
      ? Promise.reject(new Error('unavailable'))
      : original(method, params),
  );
  render(<FileBrowser session={session} online visible />);
  const title = await screen.findByText('无法读取目录');
  expect(title.closest('.file-table-state')).toHaveAttribute('role', 'alert');
  expect(title.closest('[data-slot="empty"]')).toBeInTheDocument();
  expect(screen.queryByText('此目录为空')).not.toBeInTheDocument();
});
it.each(['用户／用户组', '权限', '类型', '大小', '修改时间'])(
  'can hide and restore %s without invalidating cached rows',
  async (column) => {
    render(<FileBrowser session={session} online visible />);
    await screen.findByText('tiny.txt');
    const user = userEvent.setup();
    screen.getByRole('button', { name: '更多操作' }).focus();
    await user.keyboard('{ArrowDown}');
    await user.click(await screen.findByRole('menuitemcheckbox', { name: column }));
    await waitFor(() =>
      expect(
        screen.queryByRole('columnheader', { name: new RegExp(column) }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByText('tiny.txt')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    screen.getByRole('button', { name: '更多操作' }).focus();
    await user.keyboard('{ArrowDown}');
    await user.click(await screen.findByRole('menuitemcheckbox', { name: column }));
    expect(
      await screen.findByRole('columnheader', { name: new RegExp(column) }),
    ).toBeInTheDocument();
  },
);
it('shows shared tooltips for enabled file actions without invoking them', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  const calls = fixture.call.mock.calls.length;
  for (const label of ['返回上级', '主目录', '刷新', '上传', '新建目录', '编辑路径', '更多操作']) {
    const button = screen.getByRole('button', { name: label });
    expect(button).not.toHaveAttribute('title');
    await user.hover(button);
    expect(await screen.findByText(label)).toBeInTheDocument();
    await user.unhover(button);
    await waitFor(() => expect(screen.queryByText(label)).not.toBeInTheDocument());
  }
  expect(fixture.call).toHaveBeenCalledTimes(calls);
  await user.click(screen.getByRole('button', { name: '刷新' }));
  await waitFor(() => expect(screen.getByRole('button', { name: '刷新' })).toBeEnabled());
  await user.unhover(screen.getByRole('button', { name: '刷新' }));
  await user.hover(screen.getByRole('button', { name: '刷新' }));
  expect(await screen.findByText('刷新')).toBeInTheDocument();
}, 10000);

it('switches the directory tree action, icon and visibility together', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  const collapse = screen.getByRole('button', { name: '收起目录树' });
  expect(collapse).toHaveAttribute('aria-expanded', 'true');
  expect(collapse.querySelector('.lucide-panel-left-close')).not.toBeNull();
  await user.click(collapse);
  const expand = screen.getByRole('button', { name: '展开目录树' });
  expect(expand).toHaveAttribute('aria-expanded', 'false');
  expect(expand.querySelector('.lucide-panel-left-open')).not.toBeNull();
  expect(screen.queryByRole('treegrid', { name: '目录树' })).not.toBeInTheDocument();
  await user.click(expand);
  expect(screen.getByRole('button', { name: '收起目录树' })).toHaveAttribute(
    'aria-expanded',
    'true',
  );
});

it.each([800, 500])('keeps file actions in one location at %ipx', async (width) => {
  globalThis.ResizeObserver = class {
    constructor(private callback: ResizeObserverCallback) {}
    observe(target: Element) {
      if (target.classList.contains('file-browser'))
        this.callback(
          [{ target, contentRect: { width } } as ResizeObserverEntry],
          this as unknown as ResizeObserver,
        );
    }
    unobserve() {}
    disconnect() {}
  };
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const labels = ['上传', '下载', '新建目录'];
  for (const name of labels) {
    if (width >= 600) expect(screen.getByRole('button', { name })).toBeInTheDocument();
    else expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
  }
  const user = userEvent.setup();
  screen.getByRole('button', { name: '更多操作' }).focus();
  await user.keyboard('{ArrowDown}');
  await screen.findByRole('menuitemcheckbox', { name: '显示隐藏文件' });
  for (const name of labels) {
    if (width < 600) expect(screen.getByRole('menuitem', { name })).toBeInTheDocument();
    else expect(screen.queryByRole('menuitem', { name })).not.toBeInTheDocument();
  }
  const menu = screen.getByRole('menu');
  expect(menu.querySelectorAll('[data-slot="dropdown-menu-separator"]')).toHaveLength(
    width < 600 ? 1 : 0,
  );
});

it("leaves path editing on blur, discards the draft, and preserves the clicked control's focus", async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '编辑路径' }));
  const input = screen.getByRole('textbox', { name: '远端路径' });
  await user.clear(input);
  await user.type(input, '/unsubmitted');
  const calls = fixture.call.mock.calls.length;
  const filter = screen.getByRole('textbox', { name: '筛选当前目录' });
  await user.click(filter);
  expect(screen.queryByRole('textbox', { name: '远端路径' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'root' })).toBeInTheDocument();
  expect(filter).toHaveFocus();
  expect(fixture.call).toHaveBeenCalledTimes(calls);
  await user.click(screen.getByRole('button', { name: '编辑路径' }));
  expect(screen.getByRole('textbox', { name: '远端路径' })).toHaveValue('/root');
});

it('does not cancel an Enter-submitted navigation when the path input loses focus', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const original = fixture.call.getMockImplementation()!;
  let resolveRead!: (value: unknown) => void;
  fixture.call.mockImplementation((method: string, params: { path: string; cursor?: string }) => {
    if (method === 'listFiles' && params.path === '/var')
      return new Promise((resolve) => {
        resolveRead = resolve;
      });
    return original(method, params);
  });
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '编辑路径' }));
  const input = screen.getByRole('textbox', { name: '远端路径' });
  await user.clear(input);
  await user.type(input, '/var{Enter}');
  await waitFor(() => expect(resolveRead).toBeTypeOf('function'));
  await user.click(screen.getByRole('textbox', { name: '筛选当前目录' }));
  resolveRead({
    kind: 'directory',
    page: { path: '/var', entries: [], next: null },
  });
  expect(await screen.findByRole('button', { name: 'var' })).toBeInTheDocument();
  expect(screen.queryByRole('textbox', { name: '远端路径' })).not.toBeInTheDocument();
});

it('keeps compact breadcrumb paths clickable for both parent and root navigation', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '编辑路径' }));
  const input = screen.getByRole('textbox', { name: '远端路径' });
  await user.clear(input);
  await user.type(input, '/home/user{Enter}');
  await screen.findByRole('button', { name: 'user' });
  const breadcrumb = document.querySelector('[data-slot="path-breadcrumb"]')!;
  const displayedPath = () =>
    [...breadcrumb.querySelectorAll('button')]
      .slice(0, -1)
      .map((button) => button.textContent)
      .join('');
  expect(displayedPath()).toBe('/home/user');
  await user.click(screen.getByRole('button', { name: 'home/' }));
  await waitFor(() => expect(displayedPath()).toBe('/home'));
  expect(fixture.call).toHaveBeenCalledWith(
    'listFiles',
    expect.objectContaining({ path: '/home' }),
  );
  await user.click(screen.getByRole('button', { name: '/' }));
  await waitFor(() => expect(displayedPath()).toBe('/'));
});

it('restores remembered tree width without resizing a panel before its layout registration', async () => {
  fixture.resize.mockImplementation(() => {
    throw new Error('Layout not found for Panel');
  });
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  expect(screen.getByTestId('directory-panel')).toHaveAttribute('data-default-width', '180px');
  fixture.treeWidth = 240;
  act(() => fixture.layoutChanged?.({}, { isUserInteraction: true }));
  // The current mount keeps its default stable while the group owns live size.
  expect(screen.getByTestId('directory-panel')).toHaveAttribute('data-default-width', '180px');
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: '收起目录树' }));
  await waitFor(() => expect(screen.queryByTestId('directory-panel')).not.toBeInTheDocument());
  await user.click(screen.getByRole('button', { name: '展开目录树' }));
  expect(await screen.findByTestId('directory-panel')).toHaveAttribute(
    'data-default-width',
    '240px',
  );
  expect(fixture.resize).not.toHaveBeenCalled();
});

it('covers offline files without discarding the view, keeps panel controls usable, and removes the cover on reconnect', async () => {
  const collapse = vi.fn();
  const panelActions = <button onClick={collapse}>收起文件区</button>;
  const view = render(<FileBrowser session={session} online visible panelActions={panelActions} />);
  const file = await screen.findByText('tiny.txt');
  const filter = screen.getByRole('textbox', { name: '筛选当前目录' });
  await userEvent.setup().type(filter, 'tiny');
  view.rerender(
    <FileBrowser session={session} online={false} visible panelActions={panelActions} />,
  );
  expect(screen.getByText('连接后可用')).toBeVisible();
  expect(file).toBeInTheDocument();
  expect(file.closest('[inert]')).not.toBeNull();
  expect(filter).toHaveValue('tiny');
  expect(filter.closest('[inert]')).not.toBeNull();
  expect(screen.getByRole('button', { name: '更多操作' }).closest('[inert]')).not.toBeNull();
  const collapseButton = screen.getByRole('button', { name: '收起文件区' });
  expect(collapseButton.closest('[inert]')).toBeNull();
  await userEvent.setup().click(collapseButton);
  expect(collapse).toHaveBeenCalledOnce();
  expect(screen.queryByRole('button', { name: '重新连接' })).not.toBeInTheDocument();
  view.rerender(<FileBrowser session={session} online visible panelActions={panelActions} />);
  expect(screen.queryByText('连接后可用')).not.toBeInTheDocument();
  expect(filter.closest('[inert]')).toBeNull();
  expect(filter).toHaveValue('tiny');
});

it('closes an open file menu on disconnect and does not reopen it on reconnect', async () => {
  const view = render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  await userEvent.setup().click(screen.getByRole('button', { name: '更多操作' }));
  expect(await screen.findByRole('menu')).toBeVisible();
  view.rerender(<FileBrowser session={session} online={false} visible />);
  await waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());
  view.rerender(<FileBrowser session={session} online visible />);
  expect(screen.queryByRole('menu')).not.toBeInTheDocument();
});

it('creates a file in the right-clicked tree directory rather than the current listing', async () => {
  render(<FileBrowser session={session} online visible />);
  await screen.findByText('tiny.txt');
  const user = userEvent.setup();
  const tree = screen.getByRole('treegrid');
  await user.pointer({
    target: within(tree).getByText('/', { exact: true }),
    keys: '[MouseRight]',
  });
  screen.getByRole('menuitem', { name: '新建' }).focus();
  await user.keyboard('{ArrowRight}');
  (await screen.findByRole('menuitem', { name: '文件' })).focus();
  await user.keyboard('{Enter}');
  await user.type(screen.getByRole('textbox', { name: '名称' }), 'new.txt');
  await user.click(screen.getByRole('button', { name: '确认' }));
  await waitFor(() =>
    expect(fixture.call).toHaveBeenCalledWith('createFile', {
      sessionId: session.id,
      path: '/new.txt',
    }),
  );
});
it('blank context menu clears an earlier row target and stats the current directory', async () => {
  const original = fixture.call.getMockImplementation()!;
  fixture.call.mockImplementation(async (method, params) =>
    method === 'statFile'
      ? {
          kind: 'fileEntry',
          entry: {
            path: params.path,
            name: 'root',
            kind: 'directory',
            size: 0,
            permissions: 0o755,
            uid: 0,
            gid: 0,
            modified: null,
          },
        }
      : original(method, params),
  );
  render(<FileBrowser session={session} online visible />);
  const user = userEvent.setup();
  await user.pointer({ target: await screen.findByText('tiny.txt'), keys: '[MouseRight]' });
  expect(screen.getByRole('menuitem', { name: '重命名' })).toBeInTheDocument();
  await user.keyboard('{Escape}');
  await user.pointer({ target: document.querySelector('.file-table-wrap')!, keys: '[MouseRight]' });
  expect(screen.queryByRole('menuitem', { name: '重命名' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('menuitem', { name: '属性' }));
  await waitFor(() =>
    expect(fixture.call).toHaveBeenCalledWith('statFile', { sessionId: session.id, path: '/root' }),
  );
});

it('reuses the file browser in observation mode during control and restores manual actions on release', async () => {
  const onChanged = vi.fn(() => () => {});
  Object.assign(window, { vigilshell: { mcp: { onChanged } } });
  const controlled = {
    ...session,
    agentControl: {
      entryId: 'agent',
      entryName: 'DeepSeek Harness',
      contextId: 'ctx',
      leaseId: 'lease',
      releasing: false,
    },
  };
  const view = render(<FileBrowser session={controlled} online visible />);
  expect(await screen.findByText('tiny.txt')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '新建目录' })).toBeDisabled();
  expect(onChanged).not.toHaveBeenCalled();
  view.rerender(<FileBrowser session={session} online visible />);
  await waitFor(() => expect(screen.getByRole('button', { name: '新建目录' })).toBeEnabled());
  await userEvent.click(screen.getByRole('button', { name: '新建目录' }));
  expect(screen.getByRole('textbox', { name: '名称' })).toBeInTheDocument();
});

it('shows file task feedback in the file area, follows read directories and refreshes move destinations', async () => {
  const controlled = {
    ...session,
    agentControl: {
      entryId: 'agent',
      entryName: 'DeepSeek Harness',
      contextId: 'ctx',
      leaseId: 'lease',
      releasing: false,
    },
  };
  const task = {
    id: 'list',
    contextId: 'ctx',
    tool: 'files_read.list',
    phase: 'running',
    createdAt: 1,
    targets: [{ path: '/root', effect: 'read', recursive: false }],
  } as AgentTask;
  const view = render(<FileBrowser session={controlled} online visible agentTasks={[task]} />);
  expect(await screen.findByText('tiny.txt')).toBeInTheDocument();
  expect(screen.getByText('执行中')).toBeInTheDocument();
  expect(fixture.call).toHaveBeenCalledWith(
    'listFiles',
    expect.objectContaining({ path: '/root' }),
  );
  const moved = {
    ...task,
    id: 'move',
    tool: 'files_manage.move',
    createdAt: 2,
    phase: 'succeeded',
    targets: [
      { path: '/elsewhere/old.txt', effect: 'remove', recursive: false },
      { path: '/root/tiny.txt', effect: 'write', recursive: false },
    ],
  } as AgentTask;
  fixture.call.mockClear();
  view.rerender(<FileBrowser session={controlled} online visible agentTasks={[moved]} />);
  await waitFor(() =>
    expect(fixture.call).toHaveBeenCalledWith(
      'listFiles',
      expect.objectContaining({ path: '/root' }),
    ),
  );
  await waitFor(() =>
    expect(screen.getByRole('row', { name: /tiny.txt/ })).toHaveAttribute('aria-selected', 'true'),
  );
  expect(screen.getByText('已完成')).toBeInTheDocument();
  view.rerender(
    <FileBrowser
      session={controlled}
      online
      visible
      agentTasks={[{ ...moved, phase: 'failed', error: 'permission_denied' }]}
    />,
  );
  expect(screen.getByText('失败')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '新建目录' })).toBeDisabled();
  view.rerender(<FileBrowser session={session} online visible />);
  await waitFor(() => expect(screen.getByRole('button', { name: '新建目录' })).toBeEnabled());
  expect(screen.queryByText('失败')).not.toBeInTheDocument();
});
