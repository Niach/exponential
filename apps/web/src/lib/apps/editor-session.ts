import { TRPCError } from "@trpc/server"
import { and, desc, eq, isNull } from "drizzle-orm"
import { editorBrief } from "@exp/domain-contract/apps"
import { db } from "@/db/connection"
import { agents, apps, devices, sessions, type Agent, type App, type Session } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import { resolveSessionDevice } from "@/lib/sessions/send"
import type { Tx } from "@/lib/sessions/tx"

// VAPP-110: an app's EDITOR SESSION: ONE root per (app, member)
// (`sessions.app_id` + `user_id` = the member). #EXP-1849: it IS the
// member's conversation with the app's agent (`agent_id` too): persistent
// like every agent chat (leaving the editor ends nothing), on the agent's
// SLOT device when set, else on a device of the member's (the one named,
// else their default). A new (or an old ended) root gets the app BRIEF as
// the preface of its first message.

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

/** #EXP-1849: the app's agent, if it has one. */
async function appAgentOf(tx: Tx | typeof db, app: Pick<App, `agentId`>): Promise<Agent | null> {
  if (!app.agentId) return null
  const [agent] = await tx.select().from(agents).where(eq(agents.id, app.agentId)).limit(1)
  return agent ?? null
}

/** #EXP-1849: the device a member's app root runs on: the app agent's SLOT
 *  (its `deviceId`, a team-shared device) when set, else the member's pick. */
export function appRootDevice(agent: Pick<Agent, `deviceId`> | null, picked: string | null): string | null {
  return agent?.deviceId ? agent.deviceId : picked
}

/**
 * Create the member's editor session root (`queued` on the device) when
 * none exists. Returns the row and, for a fresh one, the host to nudge.
 * #EXP-1849 (ONE root per app and member): the root IS the member's
 * conversation with the app's agent (`agent_id` too): it runs on the agent's
 * slot when set (with the agent's runner, skills and MCP servers), else on
 * the member's own `deviceId`.
 */
export async function createEditorSessionInTx(
  tx: Tx,
  app: App,
  userId: string,
  deviceId: string,
  runner: { cli?: string | null; account?: string | null; model?: string | null }
): Promise<{ session: Session; created: { ownerId: string; deviceId: string } | null }> {
  const agent = await appAgentOf(tx, app)
  const slot = agent?.deviceId ? agent : null
  const target = slot ? slot.deviceId : deviceId
  const device = await resolveSessionDevice(userId, app.teamId, target)
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
      agentId: agent?.id ?? null,
      userId,
      deviceId: target,
      cli: slot ? slot.cli : (runner.cli ?? null),
      account: slot ? slot.account : (runner.account ?? null),
      model: slot ? slot.model : (runner.model ?? null),
      skillIds: agent && agent.skillIds.length > 0 ? agent.skillIds : null,
      mcpServerIds: agent && agent.mcpServerIds.length > 0 ? agent.mcpServerIds : null,
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
  return { session, created: { ownerId: device.ownerId, deviceId: target } }
}
