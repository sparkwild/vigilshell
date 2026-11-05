// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { translate } from '../../shared/i18n/messages';
import { WorkbenchNavigation } from './WorkbenchNavigation';
vi.mock('./DesktopProvider', () => ({
  useDesktop: () => ({ t: (key: Parameters<typeof translate>[0]) => translate(key, 'en') }),
}));
vi.mock('../components/ui/sidebar', () => ({
  SidebarContent: 'div',
  SidebarFooter: 'div',
  SidebarMenu: 'div',
  SidebarMenuItem: 'div',
  SidebarMenuButton: ({
    children,
    isActive: _active,
    ...props
  }: React.ComponentProps<'button'> & { isActive?: boolean }) => (
    <button {...props} aria-pressed={_active}>
      {children}
    </button>
  ),
}));
vi.mock('../features/connections/catalogue-view', () => ({ ConnectionNavigation: () => null }));
vi.mock('../features/mcp/model', () => ({
  useMcp: () => ({ snapshot: null }),
  navigateMcp: vi.fn(),
}));
vi.mock('../features/notifications/NotificationFeedbackProvider', () => ({
  useNotificationFeedback: () => [],
}));
afterEach(cleanup);
it('does not expose file task or editing entries in the sidebar', () => {
  render(<WorkbenchNavigation onSettings={vi.fn()} onConnected={vi.fn()} />);
  expect(screen.queryByRole('button', { name: 'File tasks' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Editing files' })).toBeNull();
  expect(screen.getAllByRole('button')).toHaveLength(3);
});
