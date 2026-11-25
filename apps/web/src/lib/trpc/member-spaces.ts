// #EXP-2161 (domain-contract member-space.json): a member's private
// Assistant on a device shared with their team. `request` picks one (the
// member), `provision` + `hosted` run it (the host's account), `forDevice` +
// `remove` list and end them (the device's owner, a team owner).
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, arrayContains, eq, inArray, isNull, ne, sql } from "drizzle-orm"
import { MEMBER_SPACE } from "@exp/domain-contract/member-space"
import { auth } from "@/lib/auth"
import { agents, devices, installs, sessions, teamMembers, users } from "@/db/schema"
import { ensureAssistant, findAssistant, syncVoiceMirror } from "@/lib/agents/assistant"
import { assertNotAgentApiKeySession } from "@/lib/auth/api-key-kind"
import { assertTeamMember } from "@/lib/auth/membership"
import { closeAskMirrorsOf, forgetWatched } from "@/lib/sessions/bus"
import { lockDeviceShares, SHARING_ORDER } from "@/lib/sessions/host"
import { relayKillTargets, treeKillTargets, type KillTarget } from "@/lib/sessions/lifecycle"
import { lockTreeRows } from "@/lib/sessions/locks"
import type { Tx } from "@/lib/sessions/tx"
import { assertOutsideSpace } from "@/lib/spaces/context"
import { deleteSpaceKeys, endSpacesInTx, finishEndedSpaces, spaceIdsOf, spacesOnHosts, type EndedSpace } from "@/lib/spaces/end"
import { approvedSpaceInstallIds, SPACE_INSTALL_KIND, spaceRows } from "@/lib/spaces/grant"
import { authedProcedure, generateTxId, router } from "@/lib/trpc"
import { nudgeDevice } from "@/lib/trpc/devices"

const uuid = z.string().uuid()
const AGENT_KEY_MESSAGE = `An agent's key cannot manage member spaces`
export const SPACE_HOST_REFUSAL = `Members cannot run their Assistant on this device through this team`
const notFound = () => new TRPCError({ code: `NOT_FOUND`, message: `Not found` })

/** A person's own credential: never an agent's key, never through a space. */
const personProcedure = authedProcedure.use(async ({ ctx, next }) => {
  assertOutsideSpace()
  await assertNotAgentApiKeySession(ctx.db, ctx.session, AGENT_KEY_MESSAGE)
  return next()
})

/** The Assistant's conversation ends, detached from it (readable, as the
 *  Assistant move kept the dropped ones): the next message starts a fresh
 *  one, whose key is its own. Its kills, read first. */
async function detachConversationInTx(tx: Tx, assistantId: string, userId: string, now: Date): Promise<KillTarget[]> {
  const roots = (
    await tx
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.agentId, assistantId), eq(sessions.userId, userId), isNull(sessions.parentId)))
  ).map((row) => row.id)
  if (roots.length === 0) return []
  for (const rootId of [...roots].sort()) await lockTreeRows(tx, rootId)
  const kills = await treeKillTargets(tx, roots)
  const ended = await tx
    .update(sessions)
    .set({
      status: `ended`,
      endedAt: sql`coalesce(${sessions.endedAt}, ${now.toISOString()}::timestamptz)`,
      busy: false,
      background: false,
      needsInput: false,
      blocked: null,
      updatedAt: now,
    })
    .where(and(inArray(sessions.rootId, roots), ne(sessions.status, `ended`)))
    .returning({ id: sessions.id })
  const ids = ended.map((row) => row.id)
  await closeAskMirrorsOf(tx, ids, now)
  await forgetWatched(tx, ids)
  await tx.update(sessions).set({ agentId: null }).where(inArray(sessions.rootId, roots))
  return kills
}

/** Both the member and the host's owner are members of `teamId` NOW; the
 *  rows are share-locked, so a removal waits and then ends what it finds. */
async function bothMembersInTx(tx: Tx, teamId: string, userIds: readonly string[]): Promise<boolean> {
  const rows = await tx
    .select({ userId: teamMembers.userId })
    .from(teamMembers)
    .where(and(eq(teamMembers.teamId, teamId), inArray(teamMembers.userId, [...userIds])))
    .for(`share`)
  return userIds.every((id) => rows.some((row) => row.userId === id))
}

export const memberSpacesRouter = router({
  /** Run my Assistant on `deviceId` (the host's device_id), through `teamId`. */
  request: personProcedure
    .input(z.object({ deviceId: z.string().min(1).max(128), teamId: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertTeamMember(userId, input.teamId)
      const result = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await lockDeviceShares(tx, input.deviceId)
        await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtextextended(${`member-space:${userId}`}, 0))`)
        // The sharing row (one account shares a machine into one team).
        const [host] = await tx
          .select()
          .from(devices)
          .where(
            and(
              eq(devices.deviceId, input.deviceId),
              isNull(devices.spaceHostId),
              arrayContains(devices.sharedTeamIds, [input.teamId])
            )
          )
          .orderBy(...SHARING_ORDER)
          .limit(1)
        if (!host) throw notFound()
        if (host.userId === userId) throw new TRPCError({ code: `BAD_REQUEST`, message: `This is your own device: pick it under your devices` })
        if (
          host.kind !== `server` ||
          !host.assistantTeamIds.includes(input.teamId) ||
          !(await bothMembersInTx(tx, input.teamId, [userId, host.userId]))
        ) {
          throw new TRPCError({ code: `FORBIDDEN`, message: SPACE_HOST_REFUSAL })
        }
        const now = new Date()
        const [current] = await tx
          .select({ id: devices.id, deviceId: devices.deviceId, spaceHostId: devices.spaceHostId, spaceTeamId: devices.spaceTeamId })
          .from(devices)
          .where(and(eq(devices.userId, userId), sql`${devices.spaceHostId} IS NOT NULL`))
          .limit(1)
        const assistant = await findAssistant(tx, userId)
        // Idempotent: already there (through another team now, if so).
        if (current && current.spaceHostId === host.id && assistant?.deviceId === current.deviceId) {
          if (current.spaceTeamId !== input.teamId) {
            await tx.update(devices).set({ spaceTeamId: input.teamId, updatedAt: now }).where(eq(devices.id, current.id))
          }
          return { txid, host, spaceId: current.id, deviceId: current.deviceId, ended: [] as EndedSpace[], kills: [] as KillTarget[] }
        }
        const ended = current ? await endSpacesInTx(tx, [current.id], `picked`) : []
        const spaceId = crypto.randomUUID()
        const deviceId = `${MEMBER_SPACE.deviceIdPrefix}${spaceId}`
        await tx.insert(devices).values({
          id: spaceId,
          userId,
          deviceId,
          label: host.label,
          kind: `server`,
          platform: host.platform,
          spaceHostId: host.id,
          spaceTeamId: input.teamId,
          // Offline until its daemon beats.
          lastSeenAt: new Date(0),
        })
        const own = assistant ?? (await ensureAssistant(userId, null, tx, deviceId))
        if (!own) throw new TRPCError({ code: `CONFLICT`, message: `Your Assistant just changed; try again.` })
        const kills = await detachConversationInTx(tx, own.id, userId, now)
        await tx.update(agents).set({ deviceId, account: null, updatedAt: now }).where(eq(agents.id, own.id))
        return { txid, host, spaceId, deviceId, ended, kills }
      })
      nudgeDevice(result.host.userId, result.host.deviceId)
      await relayKillTargets(result.kills).catch(() => {})
      await finishEndedSpaces(result.ended)
      await syncVoiceMirror(ctx.db, userId).catch((error: unknown) => console.error(`[member-spaces] voice mirror failed`, error))
      return { spaceId: result.spaceId, deviceId: result.deviceId, txid: result.txid }
    }),

  /** The host's account takes the space's key: answered once, the earlier
   *  one of that space deleted. */
  provision: personProcedure.input(z.object({ spaceId: uuid })).mutation(async ({ ctx, input }) => {
    const hosted = async () => {
      const [space] = await spaceRows(ctx.db, eq(devices.id, input.spaceId))
      return space?.live && space.hostUserId === ctx.session.user.id ? space : null
    }
    const space = await hosted()
    if (!space) throw notFound()
    const created = await auth.api.createApiKey({
      body: {
        name: `Assistant on ${space.label}`.slice(0, 180),
        userId: space.userId,
        expiresIn: null,
        rateLimitEnabled: false,
        metadata: { kind: MEMBER_SPACE.keyKind, spaceId: space.spaceId },
      },
    })
    const { invalidateSessionCache } = await import(`@/lib/auth/resolve-bearer`)
    // Ended meanwhile (its keys went with it): so does this one.
    if (!(await hosted())) {
      await deleteSpaceKeys(ctx.db, space.userId, space.spaceId)
      invalidateSessionCache()
      throw notFound()
    }
    await deleteSpaceKeys(ctx.db, space.userId, space.spaceId, { only: `space`, keepKeyId: created.id })
    invalidateSessionCache()
    const [member] = await ctx.db.select({ email: users.email }).from(users).where(eq(users.id, space.userId)).limit(1)
    return { key: created.key, userId: space.userId, email: member?.email ?? null, deviceId: space.deviceId, label: space.label }
  }),

  /** The spaces a machine of the caller's hosts: its daemon runs exactly these. */
  hosted: authedProcedure.input(z.object({ deviceId: z.string().min(1).max(128) })).query(async ({ ctx, input }) => {
    assertOutsideSpace()
    const [host] = await ctx.db
      .select({ id: devices.id })
      .from(devices)
      .where(and(eq(devices.userId, ctx.session.user.id), eq(devices.deviceId, input.deviceId), isNull(devices.spaceHostId)))
      .limit(1)
    if (!host) return { spaces: [] }
    const rows = (await spaceRows(ctx.db, spacesOnHosts([host.id]))).filter((space) => space.live)
    const names = await memberNames(ctx.db, rows.map((space) => space.userId))
    return {
      spaces: rows.map((space) => ({
        spaceId: space.spaceId,
        deviceId: space.deviceId,
        memberId: space.userId,
        memberName: names.get(space.userId) ?? ``,
        label: space.label,
      })),
    }
  }),

  /** The members with a space on a device: its owner sees all, a team owner
   *  those who run there through their team. */
  forDevice: authedProcedure.input(z.object({ id: uuid })).query(async ({ ctx, input }) => {
    assertOutsideSpace()
    const userId = ctx.session.user.id
    const rows = await manageable(ctx.db, userId, spacesOnHosts([input.id]))
    const names = await memberNames(ctx.db, rows.map((space) => space.userId))
    return {
      members: rows.map((space) => ({
        spaceId: space.spaceId,
        userId: space.userId,
        name: names.get(space.userId) ?? ``,
        teamId: space.spaceTeamId,
      })),
    }
  }),

  /** The device owner's, or a team owner's, Remove. */
  remove: personProcedure.input(z.object({ spaceId: uuid })).mutation(async ({ ctx, input }) => {
    const [space] = await manageable(ctx.db, ctx.session.user.id, eq(devices.id, input.spaceId))
    if (!space) throw notFound()
    const { txid, ended } = await ctx.db.transaction(async (tx) => {
      const id = await generateTxId(tx)
      return { txid: id, ended: await endSpacesInTx(tx, await spaceIdsOf(tx, eq(devices.id, space.spaceId)), `removed`) }
    })
    await finishEndedSpaces(ended)
    return { ok: true, txid }
  }),

  /** The caller's own space: `starting` (its daemon has not registered),
   *  `waiting` (for their approval) or `ready`; `ended` = their Assistant
   *  lost its device and asks for one. */
  mine: authedProcedure.query(async ({ ctx }) => {
    assertOutsideSpace()
    const userId = ctx.session.user.id
    const [space] = await spaceRows(ctx.db, eq(devices.userId, userId))
    const assistant = await findAssistant(ctx.db, userId)
    const ended = assistant !== null && assistant.deviceId === ``
    if (!space) return { space: null, ended }
    const [install] = await ctx.db
      .select({ id: installs.id, endpointId: installs.endpointId })
      .from(installs)
      .where(and(eq(installs.deviceId, space.spaceId), eq(installs.kind, SPACE_INSTALL_KIND), isNull(installs.revokedAt)))
      .limit(1)
    const approved = install ? (await approvedSpaceInstallIds(ctx.db, [install.id])).has(install.id) : false
    const names = await memberNames(ctx.db, [space.hostUserId])
    return {
      space: {
        spaceId: space.spaceId,
        deviceId: space.deviceId,
        label: space.label,
        hostDeviceId: space.hostDeviceId,
        ownerId: space.hostUserId,
        ownerName: names.get(space.hostUserId) ?? ``,
        teamId: space.spaceTeamId,
        teamIds: space.teamIds,
        status: !install ? (`starting` as const) : approved ? (`ready` as const) : (`waiting` as const),
        endpointId: install?.endpointId ?? null,
      },
      ended,
    }
  }),
})

type Reader = Pick<typeof import("@/db/connection").db, `select`>

async function memberNames(exec: Reader, userIds: readonly string[]): Promise<Map<string, string>> {
  if (userIds.length === 0) return new Map()
  const rows = await exec
    .select({ id: users.id, name: users.name, email: users.email })
    .from(users)
    .where(inArray(users.id, [...new Set(userIds)]))
  return new Map(rows.map((row) => [row.id, row.name || row.email]))
}

/** The spaces of `where` the caller may see and end: hosted on their device,
 *  or running through a team they own. */
async function manageable(exec: Reader, userId: string, where: Parameters<typeof spaceRows>[1]) {
  const rows = await spaceRows(exec, where)
  if (rows.length === 0) return []
  const owned = new Set(
    (
      await exec
        .select({ teamId: teamMembers.teamId })
        .from(teamMembers)
        .where(and(eq(teamMembers.userId, userId), eq(teamMembers.role, `owner`)))
    ).map((row) => row.teamId)
  )
  return rows.filter(
    (space) =>
      space.hostUserId === userId ||
      (space.spaceTeamId !== null && owned.has(space.spaceTeamId)) ||
      space.teamIds.some((teamId) => owned.has(teamId))
  )
}
