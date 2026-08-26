/**
 * `SCREENSHOT_FREEZE_NOW` — pin the seed's clock (EXP-627).
 *
 * Every seeded row is dated relative to "now" (`daysAgo`, `hoursAgo`,
 * `inDays`), which is what makes the demo data read like a live team. It also
 * means two seeds a week apart produce different ABSOLUTE dates, so the
 * screenshot store's diff-skip sees a changed pixel in every due-date chip and
 * rewrites files that document nothing new. Freezing the clock removes that
 * churn.
 *
 * OPT-IN, deliberately, and `bun run shots` does NOT set it:
 *
 *   - it only stabilises absolute dates and ordering. Relative labels
 *     ("3 hours ago") are rendered by each CLIENT against the real clock, so
 *     they drift regardless.
 *   - a stale frozen instant is actively wrong: due dates go red, devices fall
 *     out of the `onlineWindowSeconds` freshness window and photograph offline.
 *
 * Accepts epoch milliseconds or anything `Date` parses (an ISO timestamp).
 * Garbage THROWS rather than silently falling back to the real clock — a typo
 * in an env var must not quietly produce a differently-dated store.
 */
export function parseFreezeNow(raw: string | undefined): number | undefined {
  const value = raw?.trim()
  if (!value) return undefined

  // Epoch milliseconds first: `Date.parse("1767225600000")` is NaN, so the
  // numeric form has to be recognised before the date parser sees it. The sign
  // is matched too — `Date.parse("-1")` cheerfully returns a real instant in
  // 2001, so a malformed epoch must be rejected here rather than fall through.
  if (/^[+-]?\d+$/.test(value)) {
    const epoch = Number(value)
    if (!Number.isSafeInteger(epoch) || epoch <= 0) {
      throw new Error(
        `SCREENSHOT_FREEZE_NOW="${value}" is not a usable epoch in milliseconds`
      )
    }
    return epoch
  }

  const parsed = Date.parse(value)
  if (Number.isNaN(parsed)) {
    throw new Error(
      `SCREENSHOT_FREEZE_NOW="${value}" is neither epoch milliseconds nor a date ` +
        `the runtime can parse (try 2026-08-26T09:00:00Z)`
    )
  }
  return parsed
}
