import { describe, expect, it } from 'vitest';
import { encodeTerminalFrame, TerminalFrameDecoder, type TerminalFrame } from './terminal-frames';
const owner = '11111111-1111-4111-8111-111111111111',
  session = '22222222-2222-4222-8222-222222222222';
const frame: TerminalFrame = {
  kind: 3,
  owner,
  session,
  sequence: 1,
  bytes: Buffer.from('中文\u001b[31mtest'),
};
describe('terminal framing', () => {
  it('decodes every split, adjacent frames and UTF-8 bytes without loss', () => {
    const bytes = Buffer.concat([
      encodeTerminalFrame(frame),
      encodeTerminalFrame({ ...frame, sequence: 2 }),
      encodeTerminalFrame({
        ...frame,
        kind: 5,
        sequence: 2,
        bytes: Buffer.alloc(0),
      }),
    ]);
    for (let split = 1; split < bytes.length; split++) {
      const decoder = new TerminalFrameDecoder(),
        out = [...decoder.push(bytes.subarray(0, split)), ...decoder.push(bytes.subarray(split))];
      expect(out).toEqual([
        frame,
        { ...frame, sequence: 2 },
        { ...frame, kind: 5, sequence: 2, bytes: Buffer.alloc(0) },
      ]);
      expect(decoder.incomplete).toBe(false);
    }
  });
  it('rejects oversized declarations before allocating payload and detects truncated EOF', () => {
    const header = encodeTerminalFrame(frame).subarray(0, 45);
    header.writeUInt32BE(32769, 41);
    expect(() => new TerminalFrameDecoder().push(header)).toThrow('terminal_frame_too_large');
    const decoder = new TerminalFrameDecoder();
    decoder.push(encodeTerminalFrame(frame).subarray(0, 46));
    expect(decoder.incomplete).toBe(true);
  });
  it('rejects invalid kinds, owner identifiers, unsafe sequences and malformed ACK lengths', () => {
    for (const bad of [
      { ...frame, kind: 9 as 3 },
      { ...frame, sequence: 2 ** 53 },
      { ...frame, kind: 4 as const, bytes: Buffer.alloc(8) },
      { ...frame, owner: 'another-owner' },
    ])
      expect(() => encodeTerminalFrame(bad)).toThrow();
  });
});
