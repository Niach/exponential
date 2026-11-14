// #FEED-242 (@exp/emcp access.json `memberPolicy`): a member's EFFECTIVE
// level for an app = their `app_access` row, else `write` for the app's
// creator or a team owner, else the app's `default_access`. Every read path
// (the apps/app-versions/app-records shapes, tRPC, the records service, the
// broker, automations) asks here, so the rule lives in one place.
import { and, eq, inArray, sql } from "drizzle-orm"
import { memberPolicy, type Level } from "@exp/emcp/access"
import { appAccess, apps, teamMembers } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"

/** The pure rule over one app's columns and the member's row + role. */
export function levelOf(app: { createdBy: string | null; defaultAccess: string }, userId: string, role: string | null, row: string | null | undefined): Level {
  return memberPolicy((row ?? null) as Level | null, { default: app.defaultAccess as Level, exempt: app.createdBy === userId || role === `owner` })
}

/** One member's level for one app (an unknown app = `write`: the caller
 *  answers NOT_FOUND for it on its own). */
export async function appAccessLevel(executor: Pick<Tx, `select`>, appId: string, userId: string): Promise<Level> {
  const [row] = await executor
    .select({ level: appAccess.level, createdBy: apps.createdBy, defaultAccess: apps.defaultAccess, role: teamMembers.role })
    .from(apps)
    .leftJoin(appAccess, and(eq(appAccess.appId, apps.id), eq(appAccess.userId, userId)))
    .leftJoin(teamMembers, and(eq(teamMembers.teamId, apps.teamId), eq(teamMembers.userId, userId)))
    .where(eq(apps.id, appId))
    .limit(1)
  return row ? levelOf(row, userId, row.role, row.level) : `write`
}

/** Every app of the team (trashed too) → the member's level. */
export async function teamAppLevels(executor: Pick<Tx, `select`>, userId: string, teamId: string): Promise<Map<string, Level>> {
  const [member] = await executor
    .select({ role: teamMembers.role })
    .from(teamMembers)
    .where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, userId)))
    .limit(1)
  const rows = await executor
    .select({ id: apps.id, createdBy: apps.createdBy, defaultAccess: apps.defaultAccess, level: appAccess.level })
    .from(apps)
    .leftJoin(appAccess, and(eq(appAccess.appId, apps.id), eq(appAccess.userId, userId)))
    .where(eq(apps.teamId, teamId))
  return new Map(rows.map((r) => [r.id, levelOf(r, userId, member?.role ?? null, r.level)]))
}

/** One app → every listed member's level (the Settings table, Run as). */
export async function memberLevels(
  executor: Pick<Tx, `select`>,
  app: { id: string; createdBy: string | null; defaultAccess: string },
  members: readonly { userId: string; role: string }[]
): Promise<Map<string, Level>> {
  const ids = members.map((m) => m.userId)
  const rows = ids.length
    ? await executor
        .select({ userId: appAccess.userId, level: appAccess.level })
        .from(appAccess)
        .where(and(eq(appAccess.appId, app.id), inArray(appAccess.userId, ids)))
    : []
  const rowOf = new Map(rows.map((r) => [r.userId, r.level]))
  return new Map(members.map((m) => [m.userId, levelOf(app, m.userId, m.role, rowOf.get(m.userId))]))
}

/** SQL: the ids of the apps in `teams` (a uuid[] expression) the member
 *  may not see at all (effective level `none`). */
export function hiddenAppIdsQuery(userId: string, teams: ReturnType<typeof sql>) {
  return sql`
    SELECT a.id AS app_id
    FROM apps a
    LEFT JOIN team_members m ON m.team_id = a.team_id AND m.user_id = ${userId}
    LEFT JOIN app_access x ON x.app_id = a.id AND x.user_id = ${userId}
    WHERE a.team_id = ANY(${teams})
      AND COALESCE(x.level, CASE WHEN a.created_by = ${userId} OR m.role = 'owner' THEN 'write' ELSE a.default_access END) = 'none'`
}
