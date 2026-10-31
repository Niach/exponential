import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, max, or, sql } from "drizzle-orm"
import { apps, mcpCalls, mcpGrants, oauthApplications, teamMembers } from "@/db/schema"
import { users } from "@/db/auth-schema"
import { getTeamMember } from "@/lib/auth/membership"
import { revokeClient } from "@/lib/emcp/revoke"
import { authedProcedure, router } from "@/lib/trpc"

// #VAPP-184: Settings › MCP › Clients (Mine | Everyone). An MCP client
// row = one member's OAuth client with its grants (`builtin` = Exponential's
// tools, `apps` = /api/mcp/apps). Mine = every client of the caller's;
// Everyone (owners) = the clients of the team's members whose grant reaches
// this team. Revoke deletes the client's grants and tokens for that member
// (lib/emcp/revoke.ts): a member revokes their own, an owner anyone's in
// their team.

const uuid = z.string().uuid()

export interface McpClientRow {
  clientId: string
  name: string
  icon: string | null
  userId: string
  userName: string
  userEmail: string
  /** builtin | apps, in that order. */
  kinds: string[]
  /** The apps grant's Read only | Full; null without one. */
  appsAccess: string | null
  connectedAt: string
  lastUsedAt: string | null
}

/** A grant that reaches `teamId`. */
const reachesTeam = (teamId: string) =>
  or(eq(mcpGrants.allTeams, true), sql`${mcpGrants.teamIds} @> ${JSON.stringify([teamId])}::jsonb`)

export const mcpClientsRouter = router({
  list: authedProcedure
    .input(z.object({ teamId: uuid, scope: z.enum([`mine`, `everyone`]).default(`mine`) }))
    .query(async ({ ctx, input }) => {
      const me = await getTeamMember(ctx.session.user.id, input.teamId)
      if (!me) throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
      if (input.scope === `everyone` && me.role !== `owner`) {
        throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner sees everyone's MCP clients` })
      }
      const memberIds =
        input.scope === `mine`
          ? [ctx.session.user.id]
          : (await ctx.db.select({ userId: teamMembers.userId }).from(teamMembers).where(eq(teamMembers.teamId, input.teamId))).map((m) => m.userId)
      const grants = await ctx.db
        .select({
          clientId: mcpGrants.clientId,
          userId: mcpGrants.userId,
          kind: mcpGrants.kind,
          access: mcpGrants.access,
          createdAt: mcpGrants.createdAt,
          name: oauthApplications.name,
          icon: oauthApplications.icon,
          userName: users.name,
          userEmail: users.email,
        })
        .from(mcpGrants)
        .innerJoin(oauthApplications, eq(oauthApplications.clientId, mcpGrants.clientId))
        .innerJoin(users, eq(users.id, mcpGrants.userId))
        .where(and(inArray(mcpGrants.userId, memberIds), input.scope === `mine` ? undefined : reachesTeam(input.teamId)))
        .orderBy(desc(mcpGrants.createdAt))
      if (grants.length === 0) return { canSeeEveryone: me.role === `owner`, clients: [] as McpClientRow[] }
      const used = await ctx.db
        .select({ clientId: mcpCalls.clientId, userId: mcpCalls.userId, at: max(mcpCalls.createdAt) })
        .from(mcpCalls)
        .where(and(inArray(mcpCalls.userId, memberIds), inArray(mcpCalls.clientId, [...new Set(grants.map((g) => g.clientId))])))
        .groupBy(mcpCalls.clientId, mcpCalls.userId)
      const lastUsed = new Map(used.map((u) => [`${u.clientId}:${u.userId}`, u.at]))
      const rows = new Map<string, McpClientRow>()
      for (const g of grants) {
        const key = `${g.clientId}:${g.userId}`
        const row = rows.get(key) ?? {
          clientId: g.clientId,
          name: g.name,
          icon: g.icon,
          userId: g.userId,
          userName: g.userName,
          userEmail: g.userEmail,
          kinds: [],
          appsAccess: null,
          connectedAt: g.createdAt.toISOString(),
          lastUsedAt: lastUsed.get(key)?.toISOString() ?? null,
        }
        row.kinds = [...new Set([...row.kinds, g.kind])].sort()
        if (g.kind === `apps`) row.appsAccess = g.access
        if (g.createdAt.toISOString() < row.connectedAt) row.connectedAt = g.createdAt.toISOString()
        rows.set(key, row)
      }
      return { canSeeEveryone: me.role === `owner`, clients: [...rows.values()] }
    }),

  /** Revoke a client: your own (no `userId`), or (an owner) a member's
   * whose grant reaches the team. */
  revoke: authedProcedure
    .input(z.object({ clientId: z.string().min(1).max(255), teamId: uuid, userId: z.string().min(1).optional() }))
    .mutation(async ({ ctx, input }) => {
      const me = await getTeamMember(ctx.session.user.id, input.teamId)
      if (!me) throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
      const target = input.userId ?? ctx.session.user.id
      if (target !== ctx.session.user.id) {
        if (me.role !== `owner`) throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner can revoke another member's client` })
        if (!(await getTeamMember(target, input.teamId))) throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
        const [reaching] = await ctx.db
          .select({ id: mcpGrants.id })
          .from(mcpGrants)
          .where(and(eq(mcpGrants.userId, target), eq(mcpGrants.clientId, input.clientId), reachesTeam(input.teamId)))
          .limit(1)
        if (!reaching) throw new TRPCError({ code: `NOT_FOUND`, message: `Client not found` })
      }
      const revoked = await revokeClient(ctx.db, target, input.clientId)
      if (revoked === 0) throw new TRPCError({ code: `NOT_FOUND`, message: `Client not found` })
      return { ok: true as const }
    }),
})

export interface McpActivityRow {
  id: string
  tool: string
  status: string
  error: string | null
  appName: string | null
  userName: string
  clientName: string | null
  createdAt: string
}

// #VAPP-185: Settings › MCP › Activity: the `mcp_calls` audit, newest first
// (90 days by retention): owners the team's calls, a member their own.
export const mcpActivityRouter = router({
  list: authedProcedure
    .input(z.object({ teamId: uuid, limit: z.number().int().min(1).max(200).default(100) }))
    .query(async ({ ctx, input }): Promise<{ owner: boolean; calls: McpActivityRow[] }> => {
      const me = await getTeamMember(ctx.session.user.id, input.teamId)
      if (!me) throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
      const owner = me.role === `owner`
      const rows = await ctx.db
        .select({
          id: mcpCalls.id,
          tool: mcpCalls.tool,
          status: mcpCalls.status,
          error: mcpCalls.error,
          appName: apps.name,
          userName: users.name,
          clientName: oauthApplications.name,
          createdAt: mcpCalls.createdAt,
        })
        .from(mcpCalls)
        .leftJoin(apps, eq(apps.id, mcpCalls.appId))
        .leftJoin(users, eq(users.id, mcpCalls.userId))
        .leftJoin(oauthApplications, eq(oauthApplications.clientId, mcpCalls.clientId))
        .where(and(eq(mcpCalls.teamId, input.teamId), owner ? undefined : eq(mcpCalls.userId, ctx.session.user.id)))
        .orderBy(desc(mcpCalls.createdAt))
        .limit(input.limit)
      return {
        owner,
        calls: rows.map((r) => ({ ...r, userName: r.userName ?? ``, createdAt: r.createdAt.toISOString() })),
      }
    }),
})
