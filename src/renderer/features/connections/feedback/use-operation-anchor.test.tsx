// @vitest-environment jsdom
import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useFeedbackAnchor } from './use-operation-anchor';
import { OperationStore } from '../operations';
import { WorkspaceFailure } from '../workspace-failure';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it('only consumes failure feedback when the surface is visible, then remembers it after hiding', async () => {
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
  vi.stubGlobal(
    'IntersectionObserver',
    class {
      constructor(private callback: IntersectionObserverCallback) {}
      observe(target: Element) {
        this.callback(
          [
            {
              target,
              isIntersecting: true,
              intersectionRect: { width: 100, height: 30 },
            } as IntersectionObserverEntry,
          ],
          this as unknown as IntersectionObserver,
        );
      }
      disconnect() {}
    },
  );
  const store = new OperationStore({
    runtimeId: () => 'runtime',
    get: async () => ({ kind: 'done' }),
    cancel: async () => ({ kind: 'done' }),
    reconcile: async () => ({ kind: 'done' }),
  });
  function Surface({ hidden }: { hidden: boolean }) {
    const ref = useFeedbackAnchor<HTMLDivElement>('editor:a', store);
    return (
      <section inert={hidden}>
        <div
          ref={(node) => {
            if (node) node.checkVisibility = () => true;
            ref.current = node;
          }}
        >
          Failure details
        </div>
      </section>
    );
  }
  const view = render(<Surface hidden />);
  store.report(
    { owner: 'editor:a', action: 'ws_test', label: 'synthetic' },
    new WorkspaceFailure('authentication_failed'),
  );
  expect(store.forOwner('editor:a')?.presentedLocally).not.toBe(true);
  view.rerender(<Surface hidden={false} />);
  await waitFor(() => expect(store.forOwner('editor:a')?.presentedLocally).toBe(true));
  view.rerender(<Surface hidden />);
  await waitFor(() => expect(store.getSnapshot().anchors.has('editor:a')).toBe(false));
  expect(store.forOwner('editor:a')?.presentedLocally).toBe(true);
  view.unmount();
  store.dispose();
});
