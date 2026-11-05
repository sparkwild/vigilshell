import { useCallback, useRef, type Ref } from 'react';

export const overlayClassName =
  'fixed inset-0 isolate z-50 bg-(--overlay-scrim) duration-(--motion-overlay) data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0';
export const modalClassName =
  'fixed top-1/2 left-1/2 z-50 flex min-h-0 w-full max-w-[min(var(--modal-width),calc(100vw-2rem))] max-h-[calc(100dvh-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-xl bg-popover p-4 text-sm text-popover-foreground ring-1 ring-foreground/10 duration-(--motion-overlay) outline-none data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95';
export type DialogPurpose = 'notice' | 'confirm' | 'form' | 'preview';
export type DialogSize = 'compact' | 'default' | 'wide';
export const modalSizes: Record<DialogSize, string> = {
  compact: '[--modal-width:20rem]',
  default: '[--modal-width:24rem]',
  wide: '[--modal-width:42rem]',
};

export function useModalFocus(purpose: DialogPurpose, forwardedRef?: Ref<HTMLDivElement>) {
  const ref = useRef<HTMLDivElement>(null);
  const setRef = useCallback(
    (node: HTMLDivElement | null) => {
      ref.current = node;
      const cleanup = typeof forwardedRef === 'function' ? forwardedRef(node) : undefined;
      if (forwardedRef && typeof forwardedRef !== 'function') forwardedRef.current = node;
      return () => {
        ref.current = null;
        if (typeof cleanup === 'function') cleanup();
        else if (typeof forwardedRef === 'function') forwardedRef(null);
        else if (forwardedRef) forwardedRef.current = null;
      };
    },
    [forwardedRef],
  );
  const initialFocus = () => {
    const popup = ref.current;
    if (purpose !== 'form' || !popup) return popup;
    return (
      Array.from(
        popup.querySelectorAll<HTMLElement>(
          'input:not([type="hidden"]),textarea,select,[role="combobox"],[role="checkbox"],[role="switch"],[contenteditable="true"]',
        ),
      ).find(
        (element) =>
          !element.matches(':disabled,[readonly],[aria-disabled="true"],[aria-readonly="true"]') &&
          !element.closest('[hidden],[inert],[aria-hidden="true"]') &&
          getComputedStyle(element).display !== 'none' &&
          getComputedStyle(element).visibility !== 'hidden',
      ) ?? popup
    );
  };
  return { ref: setRef, initialFocus };
}
