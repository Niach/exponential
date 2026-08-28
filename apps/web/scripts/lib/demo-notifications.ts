/**
 * Pin the demo user's notification read state across a capture run (EXP-666).
 *
 * The sidebar carries an unread COUNT, and several surfaces clear notifications
 * just by being opened: issue detail fires `markReadByIssue` on mount (EXP-92)
 * and the Support surface fires `markReadSupport` (REV2-13). So the badge every
 * OTHER view photographs depends on which views ran before it — a full run walks
 * `issue-detail` (APP-5) and the `start-coding` views (APP-3) and lands on 1,
 * while a scoped run that skips them lands on 3. The committed store ended up a
 * patchwork of 1s, 2s and 3s that nothing in any merge explains, and every
 * refresh had to triage the difference by hand.
 *
 * The fix is to photograph every view against the SAME starting state: snapshot
 * the seeded read state once, before the first view, and put it back before each
 * one. What a view then does to its own badge is real product behaviour and is
 * captured truthfully — `issue-detail` still shows its notification clearing —
 * but no view can leak its side effect into the next one's frame.
 *
 * Deliberately snapshot-and-restore rather than a hard-coded expected state: the
 * snapshot IS whatever the seed produced, so this never drifts out of sync with
 * `seed-screenshots.ts`, and a run over an unseeded database restores the state
 * it actually found instead of inventing one.
 */
import { eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { notifications, users } from "@/db/schema"
import { DEMO_EMAIL } from "../screenshot-demo"

/** One row's read state, as it stood before any view was driven. */
export interface NotificationReadState {
  id: string
  readAt: Date | null
}

/**
 * The demo user's notification read state right now. Empty (not an error) when
 * the seed has not run — the caller treats a missing baseline as "nothing to
 * pin" rather than failing a lane over a badge.
 */
export async function snapshotDemoNotifications(): Promise<NotificationReadState[]> {
  const [demo] = await db
    .select({ id: users.id })
    .from(users)
    .where(eq(users.email, DEMO_EMAIL))
    .limit(1)
  if (!demo) return []
  return db
    .select({ id: notifications.id, readAt: notifications.readAt })
    .from(notifications)
    .where(eq(notifications.userId, demo.id))
}

/**
 * Put the snapshot back. Only rows whose state actually moved are written, so
 * the common case (a view that cleared nothing) costs one SELECT and no writes,
 * and Electric pushes nothing the open page has to re-render.
 */
export async function restoreDemoNotifications(
  snapshot: readonly NotificationReadState[]
): Promise<void> {
  if (snapshot.length === 0) return
  const current = new Map(
    (
      await db
        .select({ id: notifications.id, readAt: notifications.readAt })
        .from(notifications)
        .where(inArray(notifications.id, snapshot.map((row) => row.id)))
    ).map((row) => [row.id, row.readAt?.getTime() ?? null])
  )
  for (const row of snapshot) {
    const now = current.get(row.id)
    if (now === undefined) continue
    if (now === (row.readAt?.getTime() ?? null)) continue
    await db
      .update(notifications)
      .set({ readAt: row.readAt })
      .where(eq(notifications.id, row.id))
  }
}
