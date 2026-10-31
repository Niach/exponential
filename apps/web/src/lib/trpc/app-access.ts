import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { POLICY_LEVELS, memberPolicy, type Level } from "@exp/emcp/access"
import { runVia, type Expose } from "@exp/emcp/exposes"
import { appAccess, apps, mcpCredentials, mcpServers, teamMembers } from "@/db/schema"
import { users } from "@/db/auth-schema"
import { getTeamMember } from "@/lib/auth/membership"
import { upstreamState, type ConsentUpstream } from "@/lib/emcp/apps-consent"
import { connectionFor } from "@/lib/emcp/upstream/credentials"
import { authedProcedure, router } from "@/lib/trpc"

// #VAPP-183: who may use which app's MCP tools (@exp/emcp access.json).
// The member × app POLICY (Settings › MCP Access): owners set a level per
// member and app, None | Read only | Full; no row = Full; a member reads
// only their own row. The per-tool switch (an app's settings page, Tools):
// owners switch an exposed tool off for every client. The broker
// (lib/emcp/apps-broker.ts) enforces both.

const uuid = z.string().uuid()
const levelSchema = z.enum(POLICY_LEVELS as unknown as [Level, ...Level[]])

const exposesOf = (pkg: unknown): Expose[] => {
  const exposes = (pkg as { exposes?: unknown } | null)?.exposes
  return Array.isArray(exposes) ? (exposes as Expose[]) : []
}
const upstreamsOf = (pkg: unknown): string[] => {
  const upstreams = (pkg as { upstreams?: unknown } | null)?.upstreams
  return Array.isArray(upstreams) ? upstreams.filter((u): u is string => typeof u === `string`).sort() : []
}

async function memberOrNotFound(userId: string, teamId: string) {
  const member = await getTeamMember(userId, teamId)
  if (!member) throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
  return member
}

function assertOwner(role: string) {
  if (role !== `owner`) throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner can change MCP access` })
}

export interface AccessRow {
  userId: string
  name: string
  email: string
  image: string | null
  role: string
  /** app id → the member's level (Full when no row). */
  levels: Record<string, Level>
}

export const appAccessRouter = router({
  /** The Access table: the team's apps that expose tools × its members
   * (owners: everyone; a member: their own row). */
  list: authedProcedure.input(z.object({ teamId: uuid })).query(async ({ ctx, input }) => {
    const me = await memberOrNotFound(ctx.session.user.id, input.teamId)
    const appRows = await ctx.db
      .select({ id: apps.id, slug: apps.slug, name: apps.name, icon: apps.icon, color: apps.color, package: apps.package })
      .from(apps)
      .where(and(eq(apps.teamId, input.teamId), isNull(apps.deletedAt)))
      .orderBy(asc(apps.name))
    const toolApps = appRows
      .filter((a) => exposesOf(a.package).length > 0)
      .map(({ package: pkg, ...a }) => ({ ...a, toolCount: exposesOf(pkg).length }))
      .sort((a, b) => a.name.localeCompare(b.name, `en`, { sensitivity: `base` }))
    const owner = me.role === `owner`
    const members = await ctx.db
      .select({ userId: teamMembers.userId, role: teamMembers.role, name: users.name, email: users.email, image: users.image })
      .from(teamMembers)
      .innerJoin(users, eq(users.id, teamMembers.userId))
      .where(owner ? eq(teamMembers.teamId, input.teamId) : and(eq(teamMembers.teamId, input.teamId), eq(teamMembers.userId, ctx.session.user.id)))
      .orderBy(asc(users.name))
    const policies = await ctx.db
      .select({ appId: appAccess.appId, userId: appAccess.userId, level: appAccess.level })
      .from(appAccess)
      .where(owner ? eq(appAccess.teamId, input.teamId) : and(eq(appAccess.teamId, input.teamId), eq(appAccess.userId, ctx.session.user.id)))
    const levelOf = new Map(policies.map((p) => [`${p.appId}:${p.userId}`, p.level as Level]))
    const rows: AccessRow[] = members.map((m) => ({
      ...m,
      levels: Object.fromEntries(toolApps.map((a) => [a.id, memberPolicy(levelOf.get(`${a.id}:${m.userId}`))])),
    }))
    return { canEdit: owner, apps: toolApps, rows }
  }),

  /** Set one member's level for one app (owners). Full removes the row. */
  set: authedProcedure
    .input(z.object({ appId: uuid, userId: z.string().min(1), level: levelSchema }))
    .mutation(async ({ ctx, input }) => {
      const [app] = await ctx.db.select({ teamId: apps.teamId }).from(apps).where(and(eq(apps.id, input.appId), isNull(apps.deletedAt))).limit(1)
      if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
      assertOwner((await memberOrNotFound(ctx.session.user.id, app.teamId)).role)
      if (!(await getTeamMember(input.userId, app.teamId))) throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      if (input.level === `write`) {
        await ctx.db.delete(appAccess).where(and(eq(appAccess.appId, input.appId), eq(appAccess.userId, input.userId)))
      } else {
        await ctx.db
          .insert(appAccess)
          .values({ appId: input.appId, teamId: app.teamId, userId: input.userId, level: input.level })
          .onConflictDoUpdate({ target: [appAccess.appId, appAccess.userId], set: { level: input.level, updatedAt: new Date() } })
      }
      return { ok: true as const }
    }),

  /** One app's MCP side (its settings page): the exposed tools with their
   * level and switch, and the upstreams it uses with the caller's own
   * connection to each. */
  appTools: authedProcedure.input(z.object({ appId: uuid })).query(async ({ ctx, input }) => {
    const [app] = await ctx.db
      .select({ teamId: apps.teamId, package: apps.package, disabled: apps.mcpDisabledTools })
      .from(apps)
      .where(and(eq(apps.id, input.appId), isNull(apps.deletedAt)))
      .limit(1)
    if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
    const me = await memberOrNotFound(ctx.session.user.id, app.teamId)
    const names = upstreamsOf(app.package)
    const servers = names.length
      ? await ctx.db
          .select({ id: mcpServers.id, name: mcpServers.name, auth: mcpServers.auth })
          .from(mcpServers)
          .where(and(eq(mcpServers.teamId, app.teamId), inArray(mcpServers.name, names)))
      : []
    const credentials = servers.length
      ? await ctx.db
          .select()
          .from(mcpCredentials)
          .where(and(eq(mcpCredentials.userId, ctx.session.user.id), inArray(mcpCredentials.serverId, servers.map((s) => s.id))))
      : []
    const upstreams: ConsentUpstream[] = names.map((name) => {
      const server = servers.find((s) => s.name === name)
      if (!server) return { name, serverId: null, auth: null, state: null }
      return { name, serverId: server.id, auth: server.auth, state: upstreamState(connectionFor(server, credentials.find((c) => c.serverId === server.id)).status) }
    })
    const disabled = new Set(app.disabled)
    return {
      canToggle: me.role === `owner`,
      tools: exposesOf(app.package).map((e) => ({
        name: e.name,
        title: e.title,
        access: e.access,
        via: runVia(e.run),
        enabled: !disabled.has(e.name),
      })),
      upstreams,
    }
  }),

  /** Switch an exposed tool off (or back on) for every client (owners). */
  setToolEnabled: authedProcedure
    .input(z.object({ appId: uuid, tool: z.string().min(1).max(64), enabled: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const [app] = await ctx.db
        .select({ teamId: apps.teamId, disabled: apps.mcpDisabledTools })
        .from(apps)
        .where(and(eq(apps.id, input.appId), isNull(apps.deletedAt)))
        .limit(1)
      if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
      assertOwner((await memberOrNotFound(ctx.session.user.id, app.teamId)).role)
      const next = input.enabled ? app.disabled.filter((t) => t !== input.tool) : [...new Set([...app.disabled, input.tool])].sort()
      await ctx.db.update(apps).set({ mcpDisabledTools: next }).where(eq(apps.id, input.appId))
      return { ok: true as const }
    }),
})
