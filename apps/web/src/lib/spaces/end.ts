// #EXP-2161 (domain-contract member-space.json, THE END): a member space
// ends in ONE transaction with whatever withdrew it. Its keys go, its install
// is revoked, the trees it read are due for rotation, the sessions of its
// trees end, the Assistant has no device
// and the row goes. Idempotent: a space that is gone ends nothing. Callers
// run `finishEndedSpaces` after commit (kills, the host's nudge, the notice).
import { and, eq, inArray, isNotNull, isNull, like, ne, or, sql, type SQL } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { MEMBER_SPACE, MEMBER_SPACE_COPY, type SpaceEndCause } from "@exp/domain-contract/member-space"
import { apikeys } from "@/db/auth-schema"
import { agents, devices, installs, messages, sessions, sessionWatches } from "@/db/schema"
import { parseApiKeyMetadata } from "@/lib/auth/api-key-kind"
import { closeAskMirrorsOf, forgetWatched } from "@/lib/sessions/bus"
import { relayKillTargets, treeKillTargets, type KillTarget } from "@/lib/sessions/lifecycle"
import { lockTreeRows } from "@/lib/sessions/locks"
import { dropNonReaderKeysInTx, rootsHeldBy } from "@/lib/sessions/tree-keys"
import type { Tx } from "@/lib/sessions/tx"
import { isSpaceDeviceId, spaceRows, type SpaceRow } from "@/lib/spaces/grant"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

export interface EndedSpace {
  spaceId: string
  userId: string
  cause: SpaceEndCause
  label: string
  hostUserId: string
  hostDeviceId: string
  /** The conversation that ran there (the notice opens it). */
  rootId: string | null
  kills: KillTarget[]
}

const host = alias(devices, `ended_host`)

/** These trees end, their Assistant's conversation detached from it
 *  (readable, as the Assistant move kept the dropped ones): the next message
 *  starts a fresh one, with its own key and no history of this one. */
export async function detachTreesInTx(tx: Tx, rootIds: readonly string[], now: Date): Promise<string[]> {
  if (rootIds.length === 0) return []
  for (const rootId of [...rootIds].sort()) await lockTreeRows(tx, rootId)
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
    .where(and(inArray(sessions.rootId, [...rootIds]), ne(sessions.status, `ended`)))
    .returning({ id: sessions.id })
  const ids = ended.map((row) => row.id)
  await closeAskMirrorsOf(tx, ids, now)
  await forgetWatched(tx, ids)
  await tx.update(sessions).set({ agentId: null }).where(and(inArray(sessions.rootId, [...rootIds]), isNotNull(sessions.agentId)))
  return ids
}

/** The trees of a space that ends. */
async function endTreesInTx(tx: Tx, rootIds: readonly string[], now: Date): Promise<void> {
  if (rootIds.length === 0) return
  await detachTreesInTx(tx, rootIds, now)
  // Nothing the machine planted runs later, on whatever device is next: what
  // the trees wait on goes, and every unread input a session wrote there (its
  // agent speaking in its member's name too) is spent unread.
  const tree = tx.select({ id: sessions.id }).from(sessions).where(inArray(sessions.rootId, [...rootIds]))
  await tx.delete(sessionWatches).where(inArray(sessionWatches.sessionId, tree))
  await tx
    .update(messages)
    .set({ consumedAt: now, updatedAt: now })
    .where(and(inArray(messages.rootId, [...rootIds]), isNotNull(messages.authorSessionId), isNull(messages.consumedAt)))
}

/** Deletes the keys that carry `spaceId`: the space's own and the agent keys
 *  minted through it (`only` = the space's own, before a fresh one). */
export async function deleteSpaceKeys(
  exec: Pick<Tx, `select` | `delete`>,
  userId: string,
  spaceId: string,
  opts: { only?: `space`; keepKeyId?: string } = {}
): Promise<number> {
  const rows = await exec
    .select({ id: apikeys.id, metadata: apikeys.metadata })
    .from(apikeys)
    .where(and(eq(apikeys.referenceId, userId), like(apikeys.metadata, `%${spaceId}%`)))
  const doomed = rows
    .filter((row) => {
      if (row.id === opts.keepKeyId) return false
      const metadata = parseApiKeyMetadata(row.metadata)
      return metadata.spaceId === spaceId && (opts.only !== `space` || metadata.kind === `space`)
    })
    .map((row) => row.id)
  if (doomed.length > 0) await exec.delete(apikeys).where(and(eq(apikeys.referenceId, userId), inArray(apikeys.id, doomed)))
  return doomed.length
}

/** Ends these spaces (devices row ids). Returns the ones this call ended. */
export async function endSpacesInTx(tx: Tx, spaceIds: readonly string[], cause: SpaceEndCause): Promise<EndedSpace[]> {
  if (spaceIds.length === 0) return []
  // The Assistants first: `agents.update` takes its row, then the space's.
  await tx
    .select({ id: agents.id })
    .from(agents)
    .where(
      and(
        eq(agents.role, `assistant`),
        inArray(agents.ownerId, tx.select({ userId: devices.userId }).from(devices).where(and(inArray(devices.id, [...spaceIds]), isNotNull(devices.spaceHostId))))
      )
    )
    .orderBy(agents.id)
    .for(`no key update`)
  const rows = await tx
    .select({
      spaceId: devices.id,
      deviceId: devices.deviceId,
      userId: devices.userId,
      label: devices.label,
      hostRowId: devices.spaceHostId,
    })
    .from(devices)
    .where(and(inArray(devices.id, [...spaceIds]), isNotNull(devices.spaceHostId)))
    .orderBy(devices.id)
    .for(`update`)
  const out: EndedSpace[] = []
  const now = new Date()
  for (const row of rows) {
    const [hostRow] = await tx
      .select({ userId: host.userId, deviceId: host.deviceId })
      .from(host)
      .where(eq(host.id, row.hostRowId!))
      .limit(1)
    // A space's device runs the trees of its member's Assistant only.
    const trees = await tx
      .selectDistinct({ rootId: sessions.rootId })
      .from(sessions)
      .where(and(eq(sessions.deviceId, row.deviceId), eq(sessions.userId, row.userId)))
    const rootIds = trees.flatMap((tree) => (tree.rootId ? [tree.rootId] : []))
    // Read before the row goes: the kills route under it.
    const kills = (await treeKillTargets(tx, rootIds)).slice(0, 500)
    const installIds = (await tx.select({ id: installs.id }).from(installs).where(eq(installs.deviceId, row.spaceId))).map((install) => install.id)
    const held = await rootsHeldBy(tx, { installIds })
    if (installIds.length > 0) {
      await tx.update(installs).set({ revokedAt: now }).where(and(inArray(installs.id, installIds), isNull(installs.revokedAt)))
    }
    const [conversation] = rootIds.length
      ? await tx
          .select({ id: sessions.id })
          .from(sessions)
          .where(and(inArray(sessions.id, rootIds), isNull(sessions.parentId), isNotNull(sessions.agentId)))
          .limit(1)
      : []
    await endTreesInTx(tx, rootIds, now)
    await deleteSpaceKeys(tx, row.userId, row.spaceId)
    // Never a fallback to another device: the page asks for one.
    await tx
      .update(agents)
      .set({ deviceId: ``, account: null, updatedAt: now })
      .where(and(eq(agents.ownerId, row.userId), eq(agents.role, `assistant`), eq(agents.deviceId, row.deviceId)))
    await tx.delete(devices).where(eq(devices.id, row.spaceId))
    // #EXP-1951: what its install read is due for a rotation.
    await dropNonReaderKeysInTx(tx, held, { gone: new Set(installIds) })
    out.push({
      spaceId: row.spaceId,
      userId: row.userId,
      cause,
      label: row.label,
      hostUserId: hostRow?.userId ?? ``,
      hostDeviceId: hostRow?.deviceId ?? ``,
      rootId: conversation?.id ?? null,
      kills,
    })
  }
  return out
}

/** Ends the spaces of `where` that may no longer live (no team left, or the
 *  one they were picked through gone), as the rows read NOW in `tx`. */
export async function endInvalidSpacesInTx(
  tx: Tx,
  where: SQL | undefined,
  cause: SpaceEndCause | ((space: SpaceRow) => SpaceEndCause)
): Promise<EndedSpace[]> {
  const dead = (await spaceRows(tx, where)).filter((space) => !space.live)
  const out: EndedSpace[] = []
  for (const space of dead) {
    out.push(...(await endSpacesInTx(tx, [space.spaceId], typeof cause === `function` ? cause(space) : cause)))
  }
  return out
}

/** The spaces hosted by these devices rows. */
export const spacesOnHosts = (hostRowIds: readonly string[]): SQL => inArray(devices.spaceHostId, [...hostRowIds])

/** The spaces `userId` has, or hosts. */
export const spacesTouching = (userId: string): SQL =>
  or(eq(devices.userId, userId), sql`${devices.spaceHostId} IN (SELECT h.id FROM devices h WHERE h.user_id = ${userId})`)!

/** The space ids of `where`, dead or alive (an ended host, an account). */
export async function spaceIdsOf(tx: Pick<Tx, `select`>, where: SQL): Promise<string[]> {
  const rows = await tx.select({ id: devices.id }).from(devices).where(and(isNotNull(devices.spaceHostId), where))
  return rows.map((row) => row.id)
}

/** After commit, best effort: the running sessions' kills, the host's nudge
 *  (it stops the child daemon), and the member's ONE notice. */
export async function finishEndedSpaces(ended: readonly EndedSpace[]): Promise<void> {
  if (ended.length === 0) return
  try {
    const { invalidateSessionCache } = await import(`@/lib/auth/resolve-bearer`)
    invalidateSessionCache()
  } catch (error) {
    console.warn(`[spaces] session cache not cleared`, error)
  }
  const config = getSteerRelayConfig()
  for (const space of ended) {
    await relayKillTargets(space.kills).catch(() => {})
    if (config && space.hostUserId) void relayPostNudge(config, space.hostUserId, space.hostDeviceId).catch(() => {})
    if ((MEMBER_SPACE.silentCauses as string[]).includes(space.cause)) continue
    try {
      const { notifySpaceEnded } = await import(`@/lib/integrations/notifications`)
      await notifySpaceEnded({
        userId: space.userId,
        sessionId: space.rootId,
        title: MEMBER_SPACE_COPY.ended.replace(`{device}`, space.label),
      })
    } catch (error) {
      console.error(`[spaces] end notice failed`, error)
    }
  }
}

/** The watchdog's pass: a space whose teams are gone by a path that did not
 *  end it (a manual write, a race) ends within a minute. */
export async function endInvalidSpaces(): Promise<number> {
  const { db } = await import(`@/db/connection`)
  const ended = await db.transaction((tx) => endInvalidSpacesInTx(tx, undefined, `unshared`))
  await finishEndedSpaces(ended)
  return ended.length
}

export { isSpaceDeviceId }

/** An account about to be deleted in `tx`: the spaces it has and the ones
 *  it hosts (both cascade away with it). */
export async function endAccountSpacesInTx(tx: Tx, userId: string): Promise<EndedSpace[]> {
  const own = await endSpacesInTx(tx, await spaceIdsOf(tx, eq(devices.userId, userId)), `accountDeleted`)
  const hosted = await endSpacesInTx(
    tx,
    await spaceIdsOf(tx, sql`${devices.spaceHostId} IN (SELECT h.id FROM devices h WHERE h.user_id = ${userId})`),
    `ownerLeft`
  )
  return [...own, ...hosted]
}

/** A member left a team (their row went in `tx`). */
export const endLeaverSpacesInTx = (tx: Tx, userId: string): Promise<EndedSpace[]> =>
  endInvalidSpacesInTx(tx, spacesTouching(userId), (space) => (space.userId === userId ? `memberLeft` : `ownerLeft`))

/** The spaces picked through a team about to be deleted. */
export const spacesOfTeam = (tx: Pick<Tx, `select`>, teamId: string): Promise<string[]> => spaceIdsOf(tx, eq(devices.spaceTeamId, teamId))
