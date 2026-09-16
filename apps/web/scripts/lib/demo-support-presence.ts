/**
 * Pin the seeded reporter presence across a capture run (EXP-812).
 *
 * `support_threads.last_reporter_seen_at` is stamped by the REPORTER's own
 * endpoints — `/api/support/thread`, `/poll` and `/reply` all touch it on every
 * read. So capturing the `support-reporter` view (the magic-link page) rewrites
 * it to "now", and every `support-thread` frame taken afterwards renders
 * "Viewing now" or a fresher "Last seen …" than the seed produced. That line
 * pushes the whole right panel down ~29px, which is 1.19% of the frame: well
 * over tolerance, so it gets written, and the shot then flips back and forth
 * depending on whether the run re-seeded first.
 *
 * Same trade as [demo-notifications]: snapshot the seeded value once, before
 * the first view, and put it back before each one — so every view is
 * photographed against the same starting state, and a host that cannot reach
 * the database simply does not pin it.
 *
 * The browser lane must ALSO restore once at its end: `support-reporter` is
 * web-only, and the desktop and native lanes run after it.
 *
 * EXP-913: the stamp is kept as an offset from the thread's `created_at`, not
 * as an instant. `lib/demo-reclock.ts` shifts both forward as the run ages, and
 * restoring an absolute instant would undo that shift for this one column.
 */
import { eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { supportThreads, teams } from "@/db/schema"
import { TEAM_SLUG } from "../screenshot-demo"

/** One thread's reporter-presence stamp, as the seed left it. */
export interface ReporterPresenceState {
  id: string
  /** `last_reporter_seen_at - created_at` in ms; null = never seen. */
  seenOffsetMs: number | null
}

const seenOffset = (row: { createdAt: Date; lastReporterSeenAt: Date | null }) =>
  row.lastReporterSeenAt ? row.lastReporterSeenAt.getTime() - row.createdAt.getTime() : null

/**
 * The demo team's reporter-presence stamps right now. Empty (not an error)
 * when the seed has not run — the caller treats a missing baseline as "nothing
 * to pin" rather than failing a lane over a caption.
 */
export async function snapshotReporterPresence(): Promise<ReporterPresenceState[]> {
  const [team] = await db
    .select({ id: teams.id })
    .from(teams)
    .where(eq(teams.slug, TEAM_SLUG))
    .limit(1)
  if (!team) return []
  const rows = await db
    .select({
      id: supportThreads.id,
      createdAt: supportThreads.createdAt,
      lastReporterSeenAt: supportThreads.lastReporterSeenAt,
    })
    .from(supportThreads)
    .where(eq(supportThreads.teamId, team.id))
  return rows.map((row) => ({ id: row.id, seenOffsetMs: seenOffset(row) }))
}

/**
 * Put the snapshot back. Only rows whose stamp actually moved are written, so
 * the common case (a view that touched no support surface) costs one SELECT and
 * no writes.
 */
export async function restoreReporterPresence(
  snapshot: readonly ReporterPresenceState[]
): Promise<void> {
  if (snapshot.length === 0) return
  const current = new Map(
    (
      await db
        .select({
          id: supportThreads.id,
          createdAt: supportThreads.createdAt,
          lastReporterSeenAt: supportThreads.lastReporterSeenAt,
        })
        .from(supportThreads)
        .where(
          inArray(
            supportThreads.id,
            snapshot.map((row) => row.id)
          )
        )
    ).map((row) => [row.id, row])
  )
  for (const row of snapshot) {
    const thread = current.get(row.id)
    if (!thread) continue
    if (seenOffset(thread) === row.seenOffsetMs) continue
    await db
      .update(supportThreads)
      .set({
        lastReporterSeenAt:
          row.seenOffsetMs === null
            ? null
            : new Date(thread.createdAt.getTime() + row.seenOffsetMs),
      })
      .where(eq(supportThreads.id, row.id))
  }
}
