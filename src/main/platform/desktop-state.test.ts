import { defaultPreferences } from '../../shared/preferences';
import { EventEmitter } from 'node:events';
import { describe, expect, it, vi } from 'vitest';
import type { Preferences, ResolvedTheme, UpdatePreferencesRequest } from '@contracts';
import type { RpcValue } from '@contracts';
import { DesktopState as AppController } from './desktop-state';
import { BackendError, type BackendClient } from '../backend/client';

const defaults: Preferences = {
  ...defaultPreferences,
  themeMode: 'system',
  localeMode: 'system',
  revision: 0,
};
const update: UpdatePreferencesRequest = {
  patch: { themeMode: 'dark' },
  base: { themeMode: 'system' },
  expectedRevision: 0,
};
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: Error) => void;
  const promise = new Promise<T>((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
class FakeAppearance {
  systemTheme: ResolvedTheme = 'light';
  languages = ['en-US'];
  transparencyReduced = false;
  mode: Preferences['themeMode'] = 'system';
  setThemeMode(mode: Preferences['themeMode']) {
    this.mode = mode;
  }
  currentTheme() {
    return this.mode === 'system' ? this.systemTheme : this.mode;
  }
  preferredLanguages() {
    return this.languages;
  }
  reducedTransparency() {
    return this.transparencyReduced;
  }
}

function fixture() {
  const runtime = Object.assign(new EventEmitter(), {
    instanceId: 'runtime-one',
    release: vi.fn(),
    start: vi.fn().mockResolvedValue({ kind: 'ready' }),
    stop: vi.fn().mockResolvedValue(undefined),
    request: vi.fn().mockResolvedValue({ kind: 'preferences', preferences: defaults }),
    call: vi.fn(),
  });
  const appearance = new FakeAppearance();
  const controller = new AppController(runtime as unknown as BackendClient, appearance);
  return { runtime, controller, appearance };
}
describe('authoritative settings lifecycle', () => {
  it('deduplicates startup and reacts to system changes without changing stored choices', async () => {
    const { runtime, controller, appearance } = fixture();
    const p = controller.start();
    expect(controller.start()).toBe(p);
    await p;
    appearance.systemTheme = 'dark';
    appearance.languages = ['zh-CN'];
    appearance.transparencyReduced = true;
    controller.refreshAppearance();
    expect(controller.getSnapshot()).toMatchObject({
      runtime: { status: 'ready' },
      preferences: defaults,
      appearance: { theme: 'dark', locale: 'zh-CN', reducedTransparency: true },
    });
    expect(runtime.start).toHaveBeenCalledTimes(1);
    const copy = controller.getSnapshot();
    copy.preferences!.themeMode = 'dark';
    expect(controller.getSnapshot().preferences).toEqual(defaults);
  });
  it('rejects a second write instead of queuing and waits for confirmed results', async () => {
    const { runtime, controller } = fixture();
    await controller.start();
    const d = deferred<RpcValue>();
    runtime.call.mockReturnValue({ id: 'write', result: d.promise });
    const save = controller.updatePreferences(update);
    expect(controller.getSnapshot().save.phase).toBe('saving');
    expect((await controller.updatePreferences(update)).error).toBe('busy');
    expect(runtime.call).toHaveBeenCalledTimes(1);
    expect(controller.getSnapshot().preferences).toEqual(defaults);
    d.resolve({
      kind: 'updated',
      outcome: 'applied',
      preferences: { ...defaults, themeMode: 'dark', revision: 1 },
    });
    await save;
    expect(controller.getSnapshot().save.phase).toBe('idle');
  });
  it('conflicts adopt the latest authoritative value and do not replay', async () => {
    const { runtime, controller } = fixture();
    await controller.start();
    const current = { ...defaults, themeMode: 'light' as const, revision: 1 };
    runtime.call.mockReturnValue({
      id: 'write',
      result: Promise.reject(new BackendError('conflict', 'conflict', current)),
    });
    await controller.updatePreferences(update);
    expect(controller.getSnapshot()).toMatchObject({
      preferences: current,
      save: { phase: 'idle', error: 'conflict' },
    });
    expect(runtime.call).toHaveBeenCalledTimes(1);
  });
  it('confirms timeout without cancelling or restarting and preserves newer notifications', async () => {
    const { runtime, controller } = fixture();
    await controller.start();
    runtime.call.mockReturnValue({
      id: 'write',
      result: Promise.reject(new BackendError('timeout')),
    });
    runtime.request.mockResolvedValueOnce({
      kind: 'reconciled',
      preferences: { ...defaults, revision: 2 },
      outcome: {
        status: 'applied',
        preferences: { ...defaults, themeMode: 'dark', revision: 1 },
      },
    });
    await controller.updatePreferences(update);
    expect(controller.getSnapshot().preferences?.revision).toBe(2);
    expect(controller.getSnapshot().save.phase).toBe('idle');
    expect(runtime.stop).not.toHaveBeenCalled();
    expect(runtime.request).toHaveBeenLastCalledWith({
      method: 'preferences.reconcile',
      params: { requestId: 'write', runtimeId: 'runtime-one' },
    });
  });
  it('retains unknown protection until a late matching result arrives', async () => {
    const { runtime, controller } = fixture();
    await controller.start();
    runtime.call.mockReturnValue({
      id: 'write',
      result: Promise.reject(new BackendError('timeout')),
    });
    runtime.request.mockRejectedValueOnce(new BackendError('timeout'));
    await controller.updatePreferences(update);
    expect(controller.getSnapshot().save.phase).toBe('unknown');
    expect((await controller.updatePreferences(update)).error).toBe('busy');
    runtime.emit('changed', { ...defaults, themeMode: 'dark', revision: 1 });
    expect(controller.getSnapshot().save.phase).toBe('unknown');
    runtime.emit('late', {
      id: 'write',
      result: {
        kind: 'updated',
        outcome: 'applied',
        preferences: { ...defaults, themeMode: 'dark', revision: 1 },
      },
    });
    expect(controller.getSnapshot().save.phase).toBe('idle');
  });
  it('manual reconciliation with evicted evidence unblocks only after the barrier and reports unknown', async () => {
    const { runtime, controller } = fixture();
    await controller.start();
    runtime.call.mockReturnValue({
      id: 'write',
      result: Promise.reject(new BackendError('timeout')),
    });
    runtime.request.mockRejectedValueOnce(new BackendError('timeout'));
    await controller.updatePreferences(update);
    runtime.request.mockResolvedValueOnce({
      kind: 'reconciled',
      preferences: defaults,
      outcome: null,
    });
    await controller.reconcilePreferences();
    expect(controller.getSnapshot().save).toEqual({
      phase: 'idle',
      error: 'result_unknown',
    });
  });
  it('restart reads current values without claiming the original save succeeded', async () => {
    const { runtime, controller } = fixture();
    await controller.start();
    runtime.call.mockReturnValue({
      id: 'write',
      result: Promise.reject(new BackendError('runtime_unavailable')),
    });
    await controller.updatePreferences(update);
    await controller.restartBackend();
    expect(runtime.stop).toHaveBeenCalledTimes(1);
    expect(runtime.call).toHaveBeenCalledTimes(1);
    expect(controller.getSnapshot().save).toEqual({
      phase: 'idle',
      error: 'result_unknown',
    });
  });
  it.each([
    {},
    null,
    {
      patch: { themeMode: null },
      base: { themeMode: 'system' },
      expectedRevision: 0,
    },
    { patch: { themeMode: 'dark' }, base: {}, expectedRevision: 0 },
  ])('rejects invalid updates', async (value) => {
    const { controller, runtime } = fixture();
    await controller.start();
    expect((await controller.updatePreferences(value as UpdatePreferencesRequest)).error).toBe(
      'invalid_request',
    );
    expect(runtime.call).not.toHaveBeenCalled();
  });
});
