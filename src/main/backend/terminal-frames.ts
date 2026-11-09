const headerSize = 45;
export const maximumTerminalPayload = 32 * 1024;
export interface TerminalFrame {
  kind: 1 | 2 | 3 | 4 | 5;
  owner: string;
  session: string;
  sequence: number;
  bytes: Buffer;
}
function uuid(bytes: Buffer) {
  const h = bytes.toString('hex');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}
function valid(frame: TerminalFrame) {
  return (
    Number.isSafeInteger(frame.sequence) &&
    frame.sequence >= 0 &&
    frame.bytes.length <= maximumTerminalPayload &&
    ([1, 3].includes(frame.kind)
      ? frame.bytes.length > 0
      : [2, 4].includes(frame.kind)
        ? frame.bytes.length === 4
        : frame.kind === 5 && frame.bytes.length === 0)
  );
}
export function encodeTerminalFrame(frame: TerminalFrame): Buffer {
  if (!valid(frame)) throw Error('invalid_terminal_frame');
  const out = Buffer.alloc(headerSize + frame.bytes.length);
  out[0] = frame.kind;
  for (const [value, offset] of [
    [frame.owner, 1],
    [frame.session, 17],
  ] as const) {
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value))
      throw Error('invalid_terminal_owner');
    Buffer.from(value.replaceAll('-', ''), 'hex').copy(out, offset);
  }
  out.writeBigUInt64BE(BigInt(frame.sequence), 33);
  out.writeUInt32BE(frame.bytes.length, 41);
  frame.bytes.copy(out, headerSize);
  return out;
}
export class TerminalFrameDecoder {
  private buffer = Buffer.alloc(0);
  push(chunk: Buffer): TerminalFrame[] {
    const frames: TerminalFrame[] = [];
    let offset = 0;
    while (offset < chunk.length) {
      const target =
        this.buffer.length < headerSize ? headerSize : headerSize + this.buffer.readUInt32BE(41);
      if (target > headerSize + maximumTerminalPayload) throw Error('terminal_frame_too_large');
      const size = Math.min(target - this.buffer.length, chunk.length - offset);
      this.buffer = Buffer.concat([this.buffer, chunk.subarray(offset, offset + size)]);
      offset += size;
      if (this.buffer.length < headerSize) continue;
      const length = this.buffer.readUInt32BE(41);
      if (length > maximumTerminalPayload) throw Error('terminal_frame_too_large');
      if (this.buffer.length !== headerSize + length) continue;
      const frame: TerminalFrame = {
        kind: this.buffer[0] as TerminalFrame['kind'],
        owner: uuid(this.buffer.subarray(1, 17)),
        session: uuid(this.buffer.subarray(17, 33)),
        sequence: Number(this.buffer.readBigUInt64BE(33)),
        bytes: this.buffer.subarray(headerSize),
      };
      if (!valid(frame)) throw Error('invalid_terminal_frame');
      frames.push(frame);
      this.buffer = Buffer.alloc(0);
    }
    return frames;
  }
  get incomplete() {
    return this.buffer.length !== 0;
  }
}
