import { useEffect, useState } from 'react';

function available() {
  return (
    document.visibilityState === 'visible' &&
    document.hasFocus() &&
    ![
      ...document.querySelectorAll<HTMLElement>(
        '[aria-modal="true"], [data-slot="dialog-content"][data-mode="modal"]',
      ),
    ].some(
      (node) =>
        !node.hasAttribute('data-closed') &&
        !node.closest('[hidden]') &&
        (node.hasAttribute('data-open') ||
          (node.checkVisibility?.({ checkVisibilityCSS: true }) ?? !node.hidden)),
    )
  );
}

export function useAuthorizationForeground() {
  const [ready, setReady] = useState(available);
  useEffect(() => {
    const update = () => setReady(available());
    window.addEventListener('focus', update);
    window.addEventListener('blur', update);
    document.addEventListener('visibilitychange', update);
    const observer = new MutationObserver(update);
    observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: [
        'aria-modal',
        'hidden',
        'style',
        'class',
        'data-mode',
        'data-open',
        'data-closed',
      ],
    });
    update();
    return () => {
      window.removeEventListener('focus', update);
      window.removeEventListener('blur', update);
      document.removeEventListener('visibilitychange', update);
      observer.disconnect();
    };
  }, []);
  return ready;
}
