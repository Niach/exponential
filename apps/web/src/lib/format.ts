// EXP-698 — string shaping shared by the surfaces that print paths.
//
// A diff's file paths are the one place where `truncate` (a trailing ellipsis)
// destroys the only part a reader needs: the FILENAME. Middle-truncation drops
// the middle of the path instead, so `apps/web/src/…/diff-view.tsx` still names
// the file. CSS can't do this, so it is a string transform.
/**
 * Shorten `value` to at most `max` characters by replacing its middle with an
 * ellipsis, keeping both ends visible (the tail slightly longer, since that is
 * where the filename lives). Shorter values, and any `max` too small to hold a
 * character on each side of the ellipsis, are returned unchanged.
 */
export function middleTruncate(value: string, max: number): string {
  if (max <= 0) return value
  // `[...value]` so astral characters (emoji in a path) are never cut in half.
  const chars = [...value]
  if (chars.length <= max) return value
  // 1 for the ellipsis itself, and a character on each side of it.
  if (max < 3) return value
  const keep = max - 1
  const head = Math.floor(keep / 2)
  const tail = keep - head
  return `${chars.slice(0, head).join(``)}…${chars.slice(chars.length - tail).join(``)}`
}
