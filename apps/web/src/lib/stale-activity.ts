// FEED-26: a live run whose feed has gone quiet for a long time must not read
// as a healthy "Live". Every client (web, iOS, Android, desktop) mirrors this
// rule and this copy: past the threshold the header caption becomes
// `No activity for {N} min` (plus the usual ` · {device}` suffix) with the
// steady amber dot, but ONLY while nothing else explains the silence — a
// pending question/plan card, a compaction or a paused host each keep their
// own caption. "Last activity" is the moment the feed last changed while
// live (event `at` stamps are optional on the wire), so the clock is the
// viewer's own.
export const STALE_ACTIVITY_AFTER_MS = 10 * 60 * 1000

/** Whole minutes since `lastActivityAt`, or null while inside the threshold. */
export function staleActivityMinutes(
  now: number,
  lastActivityAt: number
): number | null {
  const silent = now - lastActivityAt
  if (silent < STALE_ACTIVITY_AFTER_MS) return null
  return Math.floor(silent / 60_000)
}

/** The caption: `No activity for 27 min` / `No activity for 27 min · macbook`. */
export function staleActivityLabel(
  minutes: number,
  deviceLabel: string | null | undefined
): string {
  const head = `No activity for ${minutes} min`
  return deviceLabel ? `${head} · ${deviceLabel}` : head
}
