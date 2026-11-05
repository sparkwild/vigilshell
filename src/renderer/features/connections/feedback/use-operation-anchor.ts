import { useEffect, useId, useRef } from 'react';

import type { OperationStore } from '../operations';

import { useConnections } from '../ConnectionProvider';

export function useOperationAnchor<T extends HTMLElement = HTMLSpanElement>(owner: string | null) {
  const { operations } = useConnections();
  return useFeedbackAnchor<T>(owner, operations);
}

export function useFeedbackAnchor<T extends HTMLElement>(
  owner: string | null,
  operations: Pick<OperationStore, 'setAnchor'>,
) {
  const ref = useRef<T>(null);
  const id = useId();
  useEffect(() => {
    const node = ref.current;
    if (!node || !owner) return;
    if (typeof IntersectionObserver === 'undefined') {
      operations.setAnchor(id, owner, true);
      return () => operations.setAnchor(id, owner, false);
    }
    let intersecting = false;
    const measure = () =>
      operations.setAnchor(
        id,
        owner,
        intersecting &&
          document.visibilityState === 'visible' &&
          !node.closest('[inert], [aria-hidden="true"]') &&
          node.checkVisibility({
            checkVisibilityCSS: true,
            checkOpacity: true,
          }),
      );
    const observer = new IntersectionObserver(([entry]) => {
      intersecting =
        !!entry?.isIntersecting &&
        entry.intersectionRect.width > 0 &&
        entry.intersectionRect.height > 0;
      measure();
    });
    observer.observe(node);
    document.addEventListener('visibilitychange', measure);
    const visibility = new MutationObserver(measure);
    for (let parent: Element | null = node; parent; parent = parent.parentElement)
      visibility.observe(parent, {
        attributes: true,
        attributeFilter: [
          'hidden',
          'inert',
          'aria-hidden',
          'class',
          'style',
          'data-open',
          'data-closed',
          'data-starting-style',
          'data-ending-style',
        ],
      });
    const surfaces = [
      node.closest('[data-slot="dialog-content"]'),
      node.closest('[data-slot="popover-content"][data-appearance="navigation"]'),
      node.closest('.workbench-shell'),
    ].filter((element): element is Element => !!element);
    const settled = (event: Event) => {
      if (surfaces.includes(event.target as Element)) measure();
    };
    surfaces.forEach((surface) => {
      surface.addEventListener('animationend', settled);
      surface.addEventListener('transitionend', settled);
    });
    return () => {
      surfaces.forEach((surface) => {
        surface.removeEventListener('animationend', settled);
        surface.removeEventListener('transitionend', settled);
      });
      document.removeEventListener('visibilitychange', measure);
      observer.disconnect();
      visibility.disconnect();
      operations.setAnchor(id, owner, false);
    };
  }, [id, owner, operations]);
  return ref;
}
