export function requiredInstallSpace(summary: string, compressed: number) {
  const match = /(?:^|\s)(\d+) bytes uncompressed/.exec(summary);
  const uncompressed = match ? Number(match[1]) : NaN;
  const required = uncompressed * 2 + compressed;
  if (
    !Number.isSafeInteger(uncompressed) ||
    uncompressed <= 0 ||
    !Number.isSafeInteger(required) ||
    required <= compressed
  )
    throw Error('invalid_archive');
  return required;
}
