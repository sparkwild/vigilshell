import { PassThrough, Writable } from 'node:stream';
import { describe, expect, it } from 'vitest';
import { forwardDiagnostics } from './diagnostics';

describe('diagnostic forwarding', () => {
  it('detaches after an output error while leaving the producer usable', async () => {
    const source = new PassThrough();
    const sink = new Writable({
      write(_chunk, _encoding, callback) {
        callback(new Error('EPIPE'));
      },
    });
    forwardDiagnostics(source, sink);
    source.write('first');
    await new Promise((resolve) => setImmediate(resolve));
    expect(source.listenerCount('data')).toBe(0);
    expect(() => source.write('late')).not.toThrow();
    source.destroy();
  });
  it('disposes output listeners when the producer closes', async () => {
    const source = new PassThrough();
    const sink = new PassThrough();
    forwardDiagnostics(source, sink);
    source.destroy();
    await new Promise((resolve) => setImmediate(resolve));
    expect(sink.listenerCount('error')).toBe(0);
  });
});
