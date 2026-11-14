import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { MAX_SHARE_EXPIRES_DAYS } from "@exponential_app/eui"
import { POLICY_LEVELS, type Level } from "@exp/emcp/access"
import { resolveUpstreamServer } from "@exp/emcp/upstream"
import { runVia, type Expose } from "@exp/emcp/exposes"
import { appAccess, apps, mcpCredentials, mcpServers, teamMembers } from "@/db/schema"
import { users } from "@/db/auth-schema"
import { getTeamMember } from "@/lib/auth/membership"
import { upstreamState, type ConsentUpstream } from "@/lib/emcp/apps-consent"
import { recordMcpCalls } from "@/lib/emcp/audit"
import { connectionFor } from "@/lib/emcp/upstream/credentials"
import { authedProcedure, router } from "@/lib/trpc"
import { invalidateAppShapeRules } from "@/lib/apps/records-shape"
import { appAccessLevel, levelOf, memberLevels } from "@/lib/apps/app-access-level"
import { automationRows } from "@/lib/apps/automations"
import { bindingsOf, writeUpstreamMappings } from "@/lib/apps/upstream-mapping"
import { hookRows, rotateHookSecret } from "@/lib/apps/webhooks"
import { enableShareLink, expireShareLink, revokeShareLink, rotateShareLink, shareRows } from "@/lib/apps/share-links"

// #VAPP-183: who may use which app's MCP tools (@exp/emcp access.json).
// The member × app POLICY (Settings › MCP Access): owners set a level per
// member and app, None | Read only | Full; no row = the app's default
// (#FEED-242, "Everyone else"; creator + owners Full); a member reads
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
      .select({ id: apps.id, slug: apps.slug, name: apps.name, icon: apps.icon, color: apps.color, package: apps.package, createdBy: apps.createdBy, defaultAccess: apps.defaultAccess })
      .from(apps)
      .where(and(eq(apps.teamId, input.teamId), isNull(apps.deletedAt)))
      .orderBy(asc(apps.name))
    const toolApps = appRows
      .filter((a) => exposesOf(a.package).length > 0)
      // Settings › MCP: the open Team apps row lists each app's tools.
      .map(({ package: pkg, createdBy, defaultAccess, ...a }) => ({
        ...a,
        createdBy,
        // #FEED-242: the "Everyone else" row (owners set it).
        defaultAccess: defaultAccess as Level,
        toolCount: exposesOf(pkg).length,
        tools: exposesOf(pkg).map((e) => ({ name: e.name, title: e.title || e.name, blurb: (e.description ?? ``).trim().split(/\r?\n/)[0] ?? `` })),
      }))
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
    const rowOf = new Map(policies.map((p) => [`${p.appId}:${p.userId}`, p.level as Level]))
    const rows: AccessRow[] = members.map((m) => ({
      ...m,
      levels: Object.fromEntries(toolApps.map((a) => [a.id, levelOf(a, m.userId, m.role, rowOf.get(`${a.id}:${m.userId}`))])),
    }))
    return { canEdit: owner, apps: toolApps.map(({ createdBy: _c, ...a }) => a), rows }
  }),

  /** eApps R3: the caller's OWN level for one app (no row = write): the
   *  clients disable what the records service would refuse at `read`
   *  (record-policy.json). An app at `none` is not synced to them at all. */
  mine: authedProcedure.input(z.object({ appId: uuid })).query(async ({ ctx, input }) => {
    const [app] = await ctx.db.select({ teamId: apps.teamId }).from(apps).where(and(eq(apps.id, input.appId), isNull(apps.deletedAt))).limit(1)
    if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
    await memberOrNotFound(ctx.session.user.id, app.teamId)
    return { level: await appAccessLevel(ctx.db, input.appId, ctx.session.user.id) }
  }),

  /** VAPP-198: every member's level for one app that is not Full (owners:
   *  the editor's "Run as <member>" previews under the member's access);
   *  #FEED-242: `default` = the app's "Everyone else" level. */
  levels: authedProcedure.input(z.object({ appId: uuid })).query(async ({ ctx, input }) => {
    const [app] = await ctx.db
      .select({ id: apps.id, teamId: apps.teamId, createdBy: apps.createdBy, defaultAccess: apps.defaultAccess })
      .from(apps)
      .where(and(eq(apps.id, input.appId), isNull(apps.deletedAt)))
      .limit(1)
    if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
    assertOwner((await memberOrNotFound(ctx.session.user.id, app.teamId)).role)
    const members = await ctx.db.select({ userId: teamMembers.userId, role: teamMembers.role }).from(teamMembers).where(eq(teamMembers.teamId, app.teamId))
    const levels = await memberLevels(ctx.db, app, members)
    return {
      default: app.defaultAccess as Level,
      levels: Object.fromEntries([...levels].filter(([, level]) => level !== `write`)) as Record<string, Level>,
    }
  }),

  /** #FEED-242: the app's default level for members without a row (the
   *  "Everyone else" row; owners). The creator and owners stay Full. */
  setDefault: authedProcedure.input(z.object({ appId: uuid, level: levelSchema })).mutation(async ({ ctx, input }) => {
    const [app] = await ctx.db.select({ teamId: apps.teamId }).from(apps).where(and(eq(apps.id, input.appId), isNull(apps.deletedAt))).limit(1)
    if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
    assertOwner((await memberOrNotFound(ctx.session.user.id, app.teamId)).role)
    await ctx.db.update(apps).set({ defaultAccess: input.level }).where(eq(apps.id, input.appId))
    // The app shapes follow the default at the members' next renewal.
    invalidateAppShapeRules()
    return { ok: true as const }
  }),

  /** Set one member's level for one app (owners). The level the member
   *  would have without a row removes the row (#FEED-242: Full while the
   *  default is Full, else an explicit row). */
  set: authedProcedure
    .input(z.object({ appId: uuid, userId: z.string().min(1), level: levelSchema }))
    .mutation(async ({ ctx, input }) => {
      const [app] = await ctx.db.select({ teamId: apps.teamId }).from(apps).where(and(eq(apps.id, input.appId), isNull(apps.deletedAt))).limit(1)
      if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
      assertOwner((await memberOrNotFound(ctx.session.user.id, app.teamId)).role)
      if (!(await getTeamMember(input.userId, app.teamId))) throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      const [policy] = await ctx.db.select({ createdBy: apps.createdBy, defaultAccess: apps.defaultAccess }).from(apps).where(eq(apps.id, input.appId)).limit(1)
      const target = await getTeamMember(input.userId, app.teamId)
      if (policy && input.level === levelOf(policy, input.userId, target?.role ?? null, null)) {
        await ctx.db.delete(appAccess).where(and(eq(appAccess.appId, input.appId), eq(appAccess.userId, input.userId)))
      } else {
        await ctx.db
          .insert(appAccess)
          .values({ appId: input.appId, teamId: app.teamId, userId: input.userId, level: input.level })
          .onConflictDoUpdate({ target: [appAccess.appId, appAccess.userId], set: { level: input.level, updatedAt: new Date() } })
      }
      // eApps R3: the app shapes follow the level at the member's next renewal.
      invalidateAppShapeRules()
      return { ok: true as const }
    }),

  /** One app's MCP side (its settings page): the exposed tools with their
   * level and switch, and the upstreams it uses with the caller's own
   * connection to each. */
  appTools: authedProcedure.input(z.object({ appId: uuid })).query(async ({ ctx, input }) => {
    const [app] = await ctx.db
      .select({ id: apps.id, teamId: apps.teamId, package: apps.package, publishedBy: apps.publishedBy, disabled: apps.mcpDisabledTools })
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
          .where(eq(mcpServers.teamId, app.teamId))
          .orderBy(asc(mcpServers.name))
      : []
    // #VAPP-254: the app's mapping, else exactly the upstream's name.
    const bindings = names.length ? await bindingsOf(ctx.db, input.appId) : new Map<string, string>()
    const credentials = servers.length
      ? await ctx.db
          .select()
          .from(mcpCredentials)
          .where(and(eq(mcpCredentials.userId, ctx.session.user.id), inArray(mcpCredentials.serverId, servers.map((s) => s.id))))
      : []
    const upstreams: ConsentUpstream[] = names.map((name) => {
      const server = resolveUpstreamServer(name, bindings.get(name) ?? null, servers)?.server
      if (!server) return { name, serverId: null, serverName: null, auth: null, state: null }
      return { name, serverId: server.id, serverName: server.name, auth: server.auth, state: upstreamState(connectionFor(server, credentials.find((c) => c.serverId === server.id)).status) }
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
      // #VAPP-254: what an owner may pick (apps.json `upstreamPickerCases`).
      servers: me.role === `owner` ? servers.map(({ id, name }) => ({ id, name })) : [],
      // #VAPP-220: the published package's automations with their last run
      // and last failure (their mcp_calls rows) and a live pause reason.
      automations: await automationRows(app),
      // #VAPP-237: the published package's webhooks (URL, minted or not).
      hooks: await hookRows({ id: input.appId }),
      // VAPP-241: the public links, owners only (the rows carry the guest URLs).
      shares: (await shareRows({ id: input.appId, package: app.package })).map((row) => (me.role === `owner` ? row : { ...row, url: null })),
    }
  }),

  /** #VAPP-254 (@exp/emcp upstream.json `mapping`): which team server fills
   * an upstream the app declares; owners. `serverId` null, or the server of
   * exactly that name, = the automatic default. Audited. */
  setUpstreamMapping: authedProcedure
    .input(z.object({ appId: uuid, upstream: z.string().min(1).max(64), serverId: uuid.nullable() }))
    .mutation(async ({ ctx, input }) => {
      const [app] = await ctx.db
        .select({ id: apps.id, teamId: apps.teamId, package: apps.package, draftPackage: apps.draftPackage })
        .from(apps)
        .where(and(eq(apps.id, input.appId), isNull(apps.deletedAt)))
        .limit(1)
      if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
      const me = await memberOrNotFound(ctx.session.user.id, app.teamId)
      if (me.role !== `owner`) throw new TRPCError({ code: `FORBIDDEN`, message: `Only a team owner picks an app's servers` })
      if (!upstreamsOf(app.package).includes(input.upstream) && !upstreamsOf(app.draftPackage).includes(input.upstream)) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `the app does not declare ${input.upstream}` })
      }
      await recordMcpCalls(await writeUpstreamMappings(ctx.db, app, ctx.session.user.id, [{ upstream: input.upstream, serverId: input.serverId }]))
      return { ok: true as const }
    }),

  /** #VAPP-237: mint (or rotate) a webhook's secret (owners); the secret
   *  is answered ONCE and stored only vault-sealed. */
  rotateHook: authedProcedure
    .input(z.object({ appId: uuid, name: z.string().min(1).max(40) }))
    .mutation(async ({ ctx, input }) => {
      const [app] = await ctx.db
        .select({ id: apps.id, teamId: apps.teamId, package: apps.package })
        .from(apps)
        .where(and(eq(apps.id, input.appId), isNull(apps.deletedAt)))
        .limit(1)
      if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
      await memberOrNotFound(ctx.session.user.id, app.teamId)
      return rotateHookSecret(ctx.session.user.id, app, input.name)
    }),

  /** VAPP-241: turn a public link on, rotate its token or turn it off; VAPP-247:
   *  `expire` sets a live link's expiry, `days` from now, none = never (owners). */
  setShareLink: authedProcedure
    .input(z.object({ appId: uuid, name: z.string().min(1).max(24), action: z.enum([`enable`, `rotate`, `revoke`, `expire`]), days: z.number().int().min(1).max(MAX_SHARE_EXPIRES_DAYS).nullish() }))
    .mutation(async ({ ctx, input }) => {
      const [app] = await ctx.db
        .select({ id: apps.id, teamId: apps.teamId, package: apps.package })
        .from(apps)
        .where(and(eq(apps.id, input.appId), isNull(apps.deletedAt)))
        .limit(1)
      if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
      assertOwner((await memberOrNotFound(ctx.session.user.id, app.teamId)).role)
      if (input.action === `enable`) await enableShareLink(ctx.session.user.id, app, input.name)
      else if (input.action === `rotate`) await rotateShareLink(app, input.name)
      else if (input.action === `expire`) await expireShareLink(app, input.name, input.days ?? null)
      else await revokeShareLink(app, input.name)
      return { ok: true as const }
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
