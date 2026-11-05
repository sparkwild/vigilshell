/** 候选等宽字体只在本机可用时显示；默认字体和中文回退随应用分发。 */
export const terminalFonts = [
  {
    id: 'jetbrains-mono',
    label: 'JetBrains Mono',
    family: 'JetBrains Mono Variable',
  },
  { id: 'menlo', label: 'Menlo', family: 'Menlo' },
  { id: 'monaco', label: 'Monaco', family: 'Monaco' },
  { id: 'consolas', label: 'Consolas', family: 'Consolas' },
  { id: 'cascadia-mono', label: 'Cascadia Mono', family: 'Cascadia Mono' },
  { id: 'cascadia-code', label: 'Cascadia Code', family: 'Cascadia Code' },
  {
    id: 'dejavu-sans-mono',
    label: 'DejaVu Sans Mono',
    family: 'DejaVu Sans Mono',
  },
  {
    id: 'liberation-mono',
    label: 'Liberation Mono',
    family: 'Liberation Mono',
  },
  { id: 'courier-new', label: 'Courier New', family: 'Courier New' },
] as const;
export type TerminalFont = (typeof terminalFonts)[number];
const cache = new WeakMap<Document, readonly TerminalFont[]>();
export function availableTerminalFonts(
  doc: Document = document,
  refresh = false,
): readonly TerminalFont[] {
  const previous = cache.get(doc);
  if (previous && !refresh) return previous;
  const context = doc.createElement('canvas').getContext('2d');
  const fonts = terminalFonts.filter((font) => {
    if (font.id === 'jetbrains-mono') return true;
    if (!context) return false;
    // FontFaceSet.check accepts fallback fonts. Two different fallback families
    // distinguish an installed candidate without reading local font files.
    const measure = (fallback: string, text: string) => {
      context.font = `20px ${JSON.stringify(font.family)}, ${fallback}`;
      return context.measureText(text).width;
    };
    const serif = measure('serif', 'Wi0m.@'),
      sans = measure('sans-serif', 'Wi0m.@');
    if (!serif || Math.abs(serif - sans) > 0.01) return false;
    const width = measure('serif', '0');
    return ['i', 'W', 'm', '@'].every((char) => Math.abs(measure('serif', char) - width) < 0.01);
  });
  cache.set(doc, fonts);
  return fonts;
}
export function terminalFontStack(id: string, doc: Document = document): string {
  const font =
    id === 'jetbrains-mono'
      ? terminalFonts[0]
      : (availableTerminalFonts(doc).find((font) => font.id === id) ?? terminalFonts[0]);
  const fallback = '"JetBrains Mono Variable", "Noto Sans SC Variable", monospace';
  return font.id === 'jetbrains-mono' ? fallback : `${JSON.stringify(font.family)}, ${fallback}`;
}
