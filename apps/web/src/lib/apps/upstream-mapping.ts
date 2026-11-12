// #VAPP-254 (@exp/emcp upstream.json `mapping`): an app's per-app server
// mapping: which team server fills an upstream it declares. Owners write it
// (the app's Settings, the import step); every path resolves through
// `loadRoutedServer` / `resolveUpstreamServer`. Each change is one audit row.
import { and, eq } from "drizzle-orm"
import { argsHash } from "@exp/emcp/audit"
import { upstreamMappingTool, upstreamMappingWrite } from "@exp/emcp/upstream"
import { TRPCError } from "@trpc/server"
import { db as defaultDb } from "@/db/connection"
import { appUpstreamBindings, mcpServers } from "@/db/schema"
import type { McpCallRow } from "@/lib/emcp/audit"

type Db = Pick<typeof defaultDb, `select` | `insert` | `delete`>

/** A package's declared upstream names. */
export function declaredUpstreams(pkg: unknown): string[] {
  const upstreams = (pkg as { upstreams?: unknown } | null)?.upstreams
  return Array.isArray(upstreams) ? upstreams.filter((u): u is string => typeof u === `string`) : []
}

/** The app's mapping, upstream → server id. */
export async function bindingsOf(db: Pick<Db, `select`>, appId: string): Promise<Map<string, string>> {
  const rows = await db.select({ upstream: appUpstreamBindings.upstream, serverId: appUpstreamBindings.serverId }).from(appUpstreamBindings).where(eq(appUpstreamBindings.appId, appId))
  return new Map(rows.map((row) => [row.upstream, row.serverId]))
}

/** Write an owner's picks (the caller checked the owner and the declared
 *  names). Refuses a server outside the app's team. Answers the audit rows,
 *  for the caller to record once its transaction committed. */
export async function writeUpstreamMappings(db: Db, app: { id: string; teamId: string }, userId: string, picks: readonly { upstream: string; serverId: string | null }[]): Promise<McpCallRow[]> {
  if (picks.length === 0) return []
  const servers = await db.select({ id: mcpServers.id, name: mcpServers.name }).from(mcpServers).where(eq(mcpServers.teamId, app.teamId))
  const audit: McpCallRow[] = []
  for (const pick of picks) {
    const write = upstreamMappingWrite(pick.upstream, pick.serverId, servers)
    if (`error` in write) throw new TRPCError({ code: `BAD_REQUEST`, message: `that server is not one of this team's MCP servers` })
    if (write.op === `set`) {
      await db
        .insert(appUpstreamBindings)
        .values({ appId: app.id, teamId: app.teamId, upstream: pick.upstream, serverId: write.serverId, setBy: userId })
        .onConflictDoUpdate({
          target: [appUpstreamBindings.appId, appUpstreamBindings.upstream],
          set: { serverId: write.serverId, setBy: userId, updatedAt: new Date() },
        })
    } else {
      await db.delete(appUpstreamBindings).where(and(eq(appUpstreamBindings.appId, app.id), eq(appUpstreamBindings.upstream, pick.upstream)))
    }
    const server = write.op === `set` ? servers.find((s) => s.id === write.serverId)!.name : null
    audit.push({
      teamId: app.teamId,
      userId,
      clientId: null,
      appId: app.id,
      tool: upstreamMappingTool(pick.upstream, server),
      status: `ok`,
      error: null,
      latencyMs: 0,
      argsHash: argsHash({ upstream: pick.upstream, serverId: write.op === `set` ? write.serverId : null }),
      sessionId: null,
      target: null,
    })
  }
  return audit
}
