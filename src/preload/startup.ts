/** Startup must paint before the renderer module and bundled fonts finish loading. */
export function prepareStartup(args: string[]) {
  const argument = args.find((value) => value.startsWith('--vigilshell-startup='));
  if (!argument) return;
  const appearance = JSON.parse(decodeURIComponent(argument.slice(argument.indexOf('=') + 1))) as {
    theme: string;
    locale: string;
    label: string;
  };
  const apply = () => {
    const root = document.documentElement;
    if (!root) return false;
    root.classList.toggle('dark', appearance.theme === 'dark');
    root.style.colorScheme = appearance.theme;
    root.lang = appearance.locale;
    root.style.setProperty('--startup-label', JSON.stringify(appearance.label));
    return true;
  };
  if (!apply()) {
    const observer = new MutationObserver(() => {
      if (apply()) observer.disconnect();
    });
    observer.observe(document, { childList: true });
  }
  document.addEventListener(
    'DOMContentLoaded',
    () => {
      document.getElementById('startup-screen')?.setAttribute('aria-label', appearance.label);
    },
    { once: true },
  );
}

export function finishStartup() {
  const splash = document.getElementById('startup-screen');
  if (!splash || splash.dataset.finished) return;
  splash.dataset.finished = 'true';
  // Allow the committed workspace to paint beneath the fading startup surface.
  requestAnimationFrame(() => {
    splash.classList.add('startup-finished');
    setTimeout(() => splash.remove(), 150);
  });
}
