import { expect, it } from 'vitest';
import { availableTerminalFonts, terminalFontStack } from './terminal-fonts';
function fontDocument() {
  const context = {
    font: '',
    measureText(text: string) {
      const available = this.font.includes('"Menlo"');
      return {
        width: available
          ? text.length * 12
          : text.length * (this.font.endsWith('sans-serif') ? 9 : 11),
      };
    },
  };
  return {
    createElement: () => ({ getContext: () => context }),
  } as unknown as Document;
}
it('only offers available monospace candidates and keeps the bundled fallback', () => {
  const doc = fontDocument();
  expect(availableTerminalFonts(doc).map((font) => font.id)).toEqual(['jetbrains-mono', 'menlo']);
  expect(terminalFontStack('menlo', doc)).toContain('"Menlo"');
  expect(terminalFontStack('consolas', doc)).toBe(terminalFontStack('jetbrains-mono', doc));
});
it('does not offer an unavailable font when a rendering context is unavailable', () => {
  const doc = {
    createElement: () => ({ getContext: () => null }),
  } as unknown as Document;
  expect(availableTerminalFonts(doc).map((font) => font.id)).toEqual(['jetbrains-mono']);
});
