// #EXP-1815 (notification-delivery.json CLEARING): reading a notification row
// on ANY device removes its system notification on EVERY device. Every write
// that turns rows read hands the rows it turned (RETURNING) here; the reader's
// phones get ONE silent push naming them (`dismissPayload`), which their push
// handlers match against what they still show (`dismissMatches`). Desktop
// needs none (it watches the synced `read_at`). Best effort, never throws:
// the caller's write already committed.

import { and, eq, inArray, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { notifications, sessions } from "@/db/schema"
import { sendToUsers } from "@/lib/integrations/fcm"
import {
  dismissPayload,
  notificationThread,
} from "@exp/domain-contract/notification-delivery"

export interface ReadNotificationRow {
  id: string
  type: string
  issueId: string | null
  sessionId: string | null
  deviceId: string | null
}

/** The thread of each row (a session row's chat = its root). */
async function readRows(rows: readonly ReadNotificationRow[]) {
  const sessionIds = [...new Set(rows.flatMap((row) => (row.sessionId && !row.issueId ? [row.sessionId] : [])))]
  const roots =
    sessionIds.length > 0
      ? new Map(
          (
            await db
              .select({ id: sessions.id, rootId: sessions.rootId })
              .from(sessions)
              .where(inArray(sessions.id, sessionIds))
          ).map((row) => [row.id, row.rootId])
        )
      : new Map<string, string>()
  return rows.map((row) => ({
    id: row.id,
    thread: notificationThread({
      type: row.type,
      issueId: row.issueId,
      sessionRootId: row.sessionId ? (roots.get(row.sessionId) ?? row.sessionId) : null,
      deviceId: row.deviceId,
    }),
  }))
}

export async function dismissReadNotifications(args: {
  userId: string
  rows: readonly ReadNotificationRow[]
  /** A mark-all write (optionally one team's). */
  all?: boolean
  teamId?: string | null
}): Promise<void> {
  try {
    if (args.rows.length === 0) return
    const payload = dismissPayload({
      rows: args.all ? args.rows.map((row) => ({ id: row.id, thread: `` })) : await readRows(args.rows),
      all: args.all ?? false,
      teamId: args.teamId ?? null,
    })
    if (!payload) return
    await sendToUsers([{ userId: args.userId }], { title: ``, data: payload, silent: true })
  } catch (err) {
    console.error(`[notify] dismiss push failed:`, err)
  }
}

/** Fire-and-forget form for the tRPC writes (after their commit). */
export function fireAndForgetDismiss(args: Parameters<typeof dismissReadNotifications>[0]): void {
  void dismissReadNotifications(args)
}

/** The columns a `RETURNING` hands `dismissReadNotifications`. */
export const dismissReturning = {
  id: notifications.id,
  type: notifications.type,
  issueId: notifications.issueId,
  sessionId: notifications.sessionId,
  deviceId: notifications.deviceId,
}

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

/** #EXP-1815: opening (or marking read) a session reads the reader's
 *  notification rows of it: a chat ROOT = every session of its tree, any
 *  other session = its own rows. Returns the rows it turned read. */
export async function markSessionNotificationsRead(
  tx: Tx,
  userId: string,
  opened: ReadonlyArray<{ id: string; rootId: string }>,
  now: Date
): Promise<ReadNotificationRow[]> {
  if (opened.length === 0) return []
  const roots = opened.filter((session) => session.id === session.rootId).map((session) => session.id)
  const others = opened.filter((session) => session.id !== session.rootId).map((session) => session.id)
  const treeIds =
    roots.length > 0
      ? (
          await tx
            .select({ id: sessions.id })
            .from(sessions)
            .where(inArray(sessions.rootId, roots))
        ).map((row) => row.id)
      : []
  const sessionIds = [...new Set([...treeIds, ...roots, ...others])]
  return await tx
    .update(notifications)
    .set({ readAt: now })
    .where(
      and(
        eq(notifications.userId, userId),
        isNull(notifications.readAt),
        inArray(notifications.sessionId, sessionIds)
      )
    )
    .returning(dismissReturning)
}
