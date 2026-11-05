export interface TerminalTabSize {
  id: string;
  width: number;
}
export interface TerminalTabLayout {
  widths: Map<string, number>;
  hiddenCount: number;
}

/** 先按自然宽度判断是否需要收纳，再为入口预留空间；不挤压所有标签。 */
export function terminalTabLayout(
  available: number,
  items: readonly TerminalTabSize[],
  activeId: string,
  gap = 4,
  overflowWidth = 60,
): TerminalTabLayout {
  const room = Math.max(0, Math.floor(available * 64) / 64);
  const measured = items.map((item) => ({
    id: item.id,
    full: Math.min(220, Math.max(60, Math.ceil(item.width * 64) / 64)),
  }));
  const active = measured.find((item) => item.id === activeId) ?? measured[0];
  if (!active) return { widths: new Map(), hiddenCount: 0 };
  const full = measured.reduce((sum, item) => sum + item.full, 0) + gap * (measured.length - 1);
  if (full <= room || measured.length === 1)
    return {
      widths: new Map(measured.map((item) => [item.id, Math.min(room, item.full)])),
      hiddenCount: 0,
    };
  const tabRoom = Math.max(0, room - overflowWidth);
  const selected = new Set([active.id]);
  let occupied = Math.min(tabRoom, active.full);
  for (const item of measured) {
    if (item.id === active.id) continue;
    const next = occupied + gap + item.full;
    if (next > tabRoom) break;
    selected.add(item.id);
    occupied = next;
  }
  return {
    widths: new Map(
      measured
        .filter((item) => selected.has(item.id))
        .map((item) => [item.id, Math.min(tabRoom, item.full)]),
    ),
    hiddenCount: measured.length - selected.size,
  };
}
