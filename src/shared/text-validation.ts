/** C0 control characters; intentionally distinct from the additional C1 range. */
export function hasC0Control(value: string): boolean {
  return Array.from(value).some((character) => character.charCodeAt(0) < 32);
}
