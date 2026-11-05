// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { finishStartup, prepareStartup } from './startup';

afterEach(() => {
  document.body.innerHTML = '';
  document.documentElement.removeAttribute('style');
  document.documentElement.className = '';
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it('applies the saved theme and localized loading status before renderer initialization', () => {
  document.body.innerHTML = '<div id="startup-screen" role="status"></div>';
  prepareStartup([
    `--vigilshell-startup=${encodeURIComponent(
      JSON.stringify({
        theme: 'dark',
        locale: 'zh-CN',
        label: '正在启动 VigilShell…',
      }),
    )}`,
  ]);
  document.dispatchEvent(new Event('DOMContentLoaded'));
  expect(document.documentElement.classList.contains('dark')).toBe(true);
  expect(document.documentElement.lang).toBe('zh-CN');
  expect(document.documentElement.style.colorScheme).toBe('dark');
  expect(document.getElementById('startup-screen')?.getAttribute('aria-label')).toBe(
    '正在启动 VigilShell…',
  );
});

it('removes only the startup surface after workspace paint and tolerates repeated readiness', () => {
  vi.useFakeTimers();
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => frames.push(callback));
  document.body.innerHTML = '<div id="startup-screen"></div><div id="root">Workspace</div>';
  finishStartup();
  finishStartup();
  expect(frames).toHaveLength(1);
  expect(document.getElementById('startup-screen')?.className).toBe('');
  frames[0]!(0);
  expect(document.getElementById('startup-screen')?.className).toBe('startup-finished');
  vi.advanceTimersByTime(150);
  expect(document.getElementById('startup-screen')).toBeNull();
  expect(document.getElementById('root')?.textContent).toBe('Workspace');
  finishStartup();
});
