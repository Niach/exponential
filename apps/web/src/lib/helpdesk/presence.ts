// Reporter presence for the live support chat (EXP-237). Pure module — no DB
// import — so the member-side inbox UI can share the same freshness predicate
// the server uses to gate reply emails.
//
// The /support/<token> page polls /api/support/poll every ~5s while its tab is
// visible, and every poll stamps support_threads.last_reporter_seen_at. A
// stamp fresher than this window therefore means "the reporter's browser is
// receiving replies live right now" — the reply email would only duplicate
// what the page already shows.
export const REPORTER_PRESENCE_WINDOW_MS = 30_000

// True while the reporter's tab is (very recently) heartbeating. Accepts the
// raw column value in whatever form it arrives (Date from drizzle, ISO string
// over the wire); anything unparsable counts as not viewing.
export function isReporterActivelyViewing(
  lastReporterSeenAt: Date | string | null | undefined,
  now: Date = new Date()
): boolean {
  if (!lastReporterSeenAt) return false
  const seenAt =
    lastReporterSeenAt instanceof Date
      ? lastReporterSeenAt.getTime()
      : Date.parse(lastReporterSeenAt)
  if (Number.isNaN(seenAt)) return false
  // A stamp in the future (clock skew) still reads as viewing.
  return now.getTime() - seenAt < REPORTER_PRESENCE_WINDOW_MS
}

// Lenient cursor parse for the poll endpoint: bad input degrades to null so
// the poll answers with the full message list instead of erroring.
export function parsePollSince(value: unknown): Date | null {
  if (typeof value !== `string` || !value) return null
  const parsed = Date.parse(value)
  if (Number.isNaN(parsed)) return null
  return new Date(parsed)
}
