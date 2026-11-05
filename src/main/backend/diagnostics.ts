import type { Readable, Writable } from 'node:stream';

/** Diagnostic output may disappear when a Dev launcher exits; it must not terminate the app. */
export function forwardDiagnostics(source: Readable, sink: Writable): () => void {
  let forwarding = true;
  const stop = () => {
    forwarding = false;
    source.off('data', write);
  };
  const write = (chunk: Buffer) => {
    if (!forwarding || sink.destroyed || !sink.writable) return;
    try {
      sink.write(chunk);
    } catch {
      stop();
    }
  };
  const dispose = () => {
    stop();
    sink.off('error', stop);
    source.off('close', dispose);
  };
  sink.on('error', stop);
  source.on('data', write);
  source.once('close', dispose);
  return dispose;
}
