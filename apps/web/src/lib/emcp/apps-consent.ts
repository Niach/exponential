// #VAPP-182: what the consent page offers for the apps endpoint
// (`/api/mcp/apps`, @exp/emcp access.json + upstream.json): per team of the
// member's, the apps whose head version exposes tools this member could
// see at Full, each with its tool count and the upstreams it declares (the
// member's own connection to each), plus the member's cap from the access
// policy. `grantApps` writes the `apps` row of `mcp_grants`.
import { and, asc, eq, inArray } from "drizzle-orm"
import { type GrantAccess, type Level } from "@exp/emcp/access"
import type { UpstreamState } from "@exp/emcp/upstream"
import { db } from "@/db/connection"
import { mcpCredentials, mcpGrants, mcpServers, teamMembers, teams } from "@/db/schema"
import { brokerTools } from "@/lib/emcp/apps-broker"
import { connectionFor, type McpConnectionStatus } from "@/lib/emcp/upstream/credentials"

/** A member's stored connection as an upstream.json state (`callable` =
 * connected | expired: the broker refreshes an expired token). */
export function upstreamState(status: McpConnectionStatus): UpstreamState {
  if (status === `connected` || status === `not_needed`) return `connected`
  if (status === `expired`) return `reconnect`
  if (status === `error`) return `reconnect`
  return `disconnected`
}

export interface ConsentUpstream {
  name: string
  /** The team's MCP server of that name; null = the team has none. */
  serverId: string | null
  /** How the member connects: an OAuth sign-in here, else Settings › MCP. */
  auth: string | null
  state: UpstreamState | null
}

export interface ConsentApp {
  id: string
  slug: string
  name: string
  icon: string | null
  color: string | null
  toolCount: number
  /** The member's policy for the app (#VAPP-183): `read` caps the grant at
   * Read only. */
  level: Level
  upstreams: ConsentUpstream[]
}

export interface ConsentTeam {
  id: string
  name: string
  slug: string
  apps: ConsentApp[]
}

/** The member's teams with the apps they could grant (apps without a tool
 * the member could see are left out). */
export async function appsConsentTree(userId: string): Promise<ConsentTeam[]> {
  const memberTeams = await db
    .select({ id: teams.id, name: teams.name, slug: teams.slug })
    .from(teams)
    .innerJoin(teamMembers, eq(teamMembers.teamId, teams.id))
    .where(eq(teamMembers.userId, userId))
    .orderBy(asc(teams.name))
  if (memberTeams.length === 0) return []
  const teamIds = memberTeams.map((t) => t.id)
  const servers = await db
    .select({ id: mcpServers.id, teamId: mcpServers.teamId, name: mcpServers.name, auth: mcpServers.auth })
    .from(mcpServers)
    .where(inArray(mcpServers.teamId, teamIds))
  const credentials = servers.length
    ? await db
        .select()
        .from(mcpCredentials)
        .where(and(eq(mcpCredentials.userId, userId), inArray(mcpCredentials.serverId, servers.map((s) => s.id))))
    : []
  const credentialOf = new Map(credentials.map((c) => [c.serverId, c]))

  const out: ConsentTeam[] = []
  for (const team of memberTeams) {
    const { tools } = await brokerTools(userId, {
      kind: `apps`,
      allTeams: false,
      teamIds: [team.id],
      boardIds: [],
      allApps: true,
      appIds: [],
      access: `full`,
    })
    const byApp = new Map<string, ConsentApp>()
    for (const tool of tools) {
      const existing = byApp.get(tool.app.id)
      if (existing) {
        existing.toolCount++
        continue
      }
      const upstreams = [...tool.app.upstreams].sort().map((name): ConsentUpstream => {
        const server = servers.find((s) => s.teamId === team.id && s.name === name)
        if (!server) return { name, serverId: null, auth: null, state: null }
        const status = connectionFor(server, credentialOf.get(server.id)).status
        return { name, serverId: server.id, auth: server.auth, state: upstreamState(status) }
      })
      byApp.set(tool.app.id, { id: tool.app.id, slug: tool.app.slug, name: tool.app.name, icon: tool.app.icon, color: tool.app.color, toolCount: 1, level: tool.app.level, upstreams })
    }
    out.push({ ...team, apps: [...byApp.values()].sort((a, b) => a.name.localeCompare(b.name)) })
  }
  return out
}

export interface AppsSelection {
  teamId: string
  /** Every app of the team, including apps created later. */
  allApps: boolean
  appIds: string[]
  access: GrantAccess
}

/** The selection clamped to the member's team and its offered apps; null =
 * nothing grantable (not a member, no app chosen). */
export async function clampAppsSelection(userId: string, input: AppsSelection): Promise<AppsSelection | null> {
  const tree = await appsConsentTree(userId)
  const team = tree.find((t) => t.id === input.teamId)
  if (!team) return null
  if (input.allApps) return { teamId: team.id, allApps: true, appIds: [], access: input.access }
  const offered = new Set(team.apps.map((a) => a.id))
  const appIds = [...new Set(input.appIds)].filter((id) => offered.has(id)).sort()
  return appIds.length ? { teamId: team.id, allApps: false, appIds, access: input.access } : null
}

/** Write (or replace) the client's `apps` grant. */
export async function writeAppsGrant(userId: string, clientId: string, selection: AppsSelection): Promise<void> {
  const values = {
    allTeams: false,
    teamIds: [selection.teamId],
    boardIds: [],
    allApps: selection.allApps,
    appIds: selection.appIds,
    access: selection.access,
  }
  await db
    .insert(mcpGrants)
    .values({ userId, clientId, kind: `apps`, ...values })
    .onConflictDoUpdate({
      target: [mcpGrants.userId, mcpGrants.clientId, mcpGrants.kind],
      set: { ...values, updatedAt: new Date() },
    })
}
