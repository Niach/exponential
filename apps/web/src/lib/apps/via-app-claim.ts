// VAPP-231 (via-app.json `claim`): the server's check of a host's claim that a
// write comes from its app. A claim only adds a caption: it never widens
// access, and every refusal (this file never throws) lands the write with no
// `via_app_id`.
import { and, eq, isNull } from "drizzle-orm"
import { bridgePackage } from "@exponential_app/eui"
import { parseViaAppHeader, VIA_APP_HEADER, viaAppAccepted, type ViaAppCall, type ViaAppClaim } from "@exp/domain-contract/via-app"
import { db as defaultDb } from "@/db/connection"
import { apps, teamMembers } from "@/db/schema"
import { appAccessLevel } from "@/lib/apps/records-service"
import { toolCallsOf } from "@/lib/emcp/audit"
import { requestViaApp } from "@/lib/emcp/via-app"
import type { McpAccess } from "@/lib/emcp/scope"

type Executor = Pick<typeof defaultDb, `select`>

/** The claimed app's id when the claim holds for every call, else null. */
export async function verifiedViaApp(
  userId: string,
  claim: ViaAppClaim,
  calls: readonly ViaAppCall[],
  executor: Executor = defaultDb,
  reaches: (teamId: string) => boolean = () => true
): Promise<string | null> {
  try {
    const [app] = await executor
      .select({ id: apps.id, teamId: apps.teamId, package: apps.package, publishedVersion: apps.publishedVersion })
      .from(apps)
      .where(and(eq(apps.id, claim.appId), isNull(apps.deletedAt)))
      .limit(1)
    if (!app || !reaches(app.teamId)) return null
    const [member] = await executor
      .select({ userId: teamMembers.userId })
      .from(teamMembers)
      .where(and(eq(teamMembers.teamId, app.teamId), eq(teamMembers.userId, userId)))
      .limit(1)
    if (!member) return null
    const access = await appAccessLevel(executor, app.id, userId)
    // Round 9 §14: a stored format 5 package names `records.upsert`, `harness.mcp`.
    const functions = bridgePackage(app.package as { functions?: unknown } | null)?.functions
    const accepted = calls.every((call) => viaAppAccepted({ version: claim.version, publishedVersion: app.publishedVersion, access, functions, call }))
    return accepted ? app.id : null
  } catch (error) {
    console.error(`[via-app] could not check a claim:`, error)
    return null
  }
}

/** An MCP request tagged with the app its header claims, when the claim holds
 *  for every `tools/call` in its body; the request itself otherwise. */
export async function requestWithViaAppClaim(request: Request, userId: string, access: McpAccess): Promise<Request> {
  const claim = parseViaAppHeader(request.headers.get(VIA_APP_HEADER))
  if (!claim) return request
  const body: unknown = await request.clone().json().catch(() => null)
  const calls = toolCallsOf(body).map((call): ViaAppCall => {
    const op = (call.args as { op?: unknown } | null | undefined)?.op
    // #VAPP-293: a noun's `_write` twin claims as the noun.
    return { tool: call.tool.replace(/_write$/, ``), op: typeof op === `string` ? op : null }
  })
  if (calls.length === 0) return request
  const appId = await verifiedViaApp(userId, claim, calls, defaultDb, (teamId) => access.full || access.visibleTeamIds.has(teamId))
  return appId ? requestViaApp(request, appId) : request
}
