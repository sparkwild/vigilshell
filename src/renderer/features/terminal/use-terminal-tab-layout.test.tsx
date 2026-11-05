// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useTerminalTabLayout } from './use-terminal-tab-layout';
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
it('reserves space for a masked agent logo so an otherwise fitting label is not truncated', () => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
    this: HTMLElement,
  ) {
    return new DOMRect(0, 0, this.classList.contains('terminal-tab-label-measure') ? 80 : 0, 16);
  });
  function View({ masked }: { masked: boolean }) {
    const layout = useTerminalTabLayout(['tab'], 'tab', masked ? 'agent-logo' : 'svg-icon');
    return (
      <div>
        <div ref={layout.viewport}>
          <div ref={layout.list}>
            <div
              role="tab"
              data-session-id="tab"
              data-width={layout.widths.get('tab')}
              style={{ paddingLeft: 8, paddingRight: 8, columnGap: 4 }}
            >
              {masked ? (
                <span className="mcp-client-logo" style={{ width: 20 }} />
              ) : (
                <svg style={{ width: 20 }} />
              )}
              <span className="terminal-tab-label-measure">阿里云服务器</span>
            </div>
          </div>
        </div>
      </div>
    );
  }
  const view = render(<View masked={false} />);
  const svgWidth = screen.getByRole('tab').getAttribute('data-width');
  view.rerender(<View masked />);
  expect(screen.getByRole('tab').getAttribute('data-width')).toBe(svgWidth);
});
