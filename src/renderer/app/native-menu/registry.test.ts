import { expect, it, vi } from 'vitest';
import { NativeMenuRegistry } from './registry';

it('dispatches to exactly one active UI owner and restores the underlying owner after leaving a page', () => {
  const registry = new NativeMenuRegistry(),
    tab = vi.fn(),
    page = vi.fn();
  registry.set('workspace', 0, { close: { enabled: true, closeTarget: 'tab', run: tab } });
  registry.set('settings', 2, { close: { enabled: true, closeTarget: 'page', run: page } });
  expect(registry.snapshot().closeTarget).toBe('page');
  registry.dispatch('close');
  expect(page).toHaveBeenCalledOnce();
  expect(tab).not.toHaveBeenCalled();
  registry.remove('settings');
  registry.dispatch('close');
  expect(tab).toHaveBeenCalledOnce();
});

it('uses the latest selected target and rejects an action that became unavailable', () => {
  const registry = new NativeMenuRegistry(),
    first = vi.fn(),
    second = vi.fn();
  registry.set('connections', 0, { connect: { enabled: true, run: first } });
  registry.set('connections', 0, {
    connect: { enabled: true, run: second },
    toggleSidebar: { enabled: true, checked: true, run: vi.fn() },
  });
  registry.dispatch('connect');
  expect(second).toHaveBeenCalledOnce();
  expect(first).not.toHaveBeenCalled();
  expect(registry.snapshot().checked).toEqual(['toggleSidebar']);
  registry.set('connections', 0, { connect: { enabled: false, run: second } });
  registry.dispatch('connect');
  expect(second).toHaveBeenCalledOnce();
  expect(registry.snapshot().enabled).toEqual([]);
});
