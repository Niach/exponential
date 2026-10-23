import { TRPCError } from "@trpc/server"
import { and, desc, eq, isNull } from "drizzle-orm"
import { editorBrief } from "@exp/domain-contract/apps"
import { db } from "@/db/connection"
import { apps, devices, sessions, type App, type Session } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import { resolveSessionDevice } from "@/lib/sessions/send"
import type { Tx } from "@/lib/sessions/tx"

// VAPP-110: an app's EDITOR SESSION: ONE root per (app, member)
// (`sessions.app_id` + `user_id` = the member), on a device of the member's
// (the one named, else their default), live only while the member has the
// editor open: leaving ENDS it (`sessions.end` on the root, the tree), the
// next message revives the same root (FEED-134 `reviveRoot`), so the chat is
// the root's messages. A new or revived session gets the app BRIEF as the
// preface of its first message.

/** The app, when the caller is a member of its team and it is not trashed. */
export async function loadAppForUser(userId: string, appId: string): Promise<App> {
  const [app] = await db.select().from(apps).where(eq(apps.id, appId)).limit(1)
  if (!app || app.deletedAt || !(await getTeamMember(userId, app.teamId))) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  }
  return app
}

/** The member's editor session root for the app, if it exists. */
export async function findEditorSession(
  tx: Tx | typeof db,
  appId: string,
  userId: string
): Promise<Session | null> {
  const [row] = await tx
    .select()
    .from(sessions)
    .where(and(eq(sessions.appId, appId), eq(sessions.userId, userId), isNull(sessions.parentId)))
    .limit(1)
  return row ?? null
}

/** The member's own device to run the editor on: their default one, else
 *  the one seen last. */
export async function defaultDeviceFor(userId: string): Promise<string | null> {
  const rows = await db
    .select({ deviceId: devices.deviceId, isDefault: devices.isDefault, lastSeenAt: devices.lastSeenAt })
    .from(devices)
    .where(eq(devices.userId, userId))
    .orderBy(desc(devices.isDefault), desc(devices.lastSeenAt))
    .limit(1)
  return rows[0]?.deviceId ?? null
}

/** The brief + the person's text: the first message of a fresh or continued
 *  editor session. */
export function briefedText(app: App, text: string | undefined): string {
  return [editorBrief(app), text?.trim() ?? ``].filter(Boolean).join(`\n\n`)
}

/**
 * Create the member's editor session root (`queued` on the device) when
 * none exists. Returns the row and, for a fresh one, the host to nudge.
 */
export async function createEditorSessionInTx(
  tx: Tx,
  app: App,
  userId: string,
  deviceId: string,
  runner: { cli?: string | null; account?: string | null; model?: string | null }
): Promise<{ session: Session; created: { ownerId: string; deviceId: string } | null }> {
  const device = await resolveSessionDevice(userId, app.teamId, deviceId)
  const id = crypto.randomUUID()
  const [session] = await tx
    .insert(sessions)
    .values({
      id,
      teamId: app.teamId,
      parentId: null,
      rootId: id,
      depth: 0,
      appId: app.id,
      userId,
      deviceId,
      cli: runner.cli ?? null,
      account: runner.account ?? null,
      model: runner.model ?? null,
      repositoryId: null,
      status: `queued`,
      shared: device.shared,
      title: app.name,
    })
    .onConflictDoNothing()
    .returning()
  if (!session) {
    const raced = await findEditorSession(tx, app.id, userId)
    if (!raced) throw new TRPCError({ code: `CONFLICT`, message: `The editor session just changed; try again.` })
    return { session: raced, created: null }
  }
  return { session, created: { ownerId: device.ownerId, deviceId } }
}
