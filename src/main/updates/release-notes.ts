import type { AvailableRelease } from '@contracts';
import {
  normalizeReleaseNotes,
  releaseNotesFromDocument,
  releaseNotesLimit,
} from '../../shared/release-notes';

/** Optional release copy must never decide whether a verified update can be downloaded. */
export async function loadReleaseNotes(
  release: AvailableRelease,
  url: string,
  fetcher: typeof fetch,
) {
  const original = normalizeReleaseNotes(release);
  try {
    const response = await fetcher(url, {
      credentials: 'omit',
      signal: AbortSignal.timeout(5000),
      headers: { Accept: 'application/json' },
    });
    if (!response.ok || Number(response.headers.get('content-length')) > releaseNotesLimit) {
      await response.body?.cancel();
      return original;
    }
    const reader = response.body?.getReader();
    if (!reader) return original;
    const chunks: Uint8Array[] = [];
    let length = 0;
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        length += value.byteLength;
        if (length > releaseNotesLimit) {
          await reader.cancel();
          return original;
        }
        chunks.push(value);
      }
    } finally {
      reader.releaseLock();
    }
    const localizedNotes = releaseNotesFromDocument(
      JSON.parse(Buffer.concat(chunks).toString('utf8')),
      release.version,
      release.tag,
    );
    return localizedNotes ? { ...original, localizedNotes } : original;
  } catch {
    return original;
  }
}
