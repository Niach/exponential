// EXP-1458: the core of a tree key write and the readers rule, shared by
// the tRPC `sessionKeys` router and `sessions.send {seal}` (a new root's key
// in its own transaction). Router-free: lib/sessions never imports a router
// (suites mock @/lib/trpc whole).
import { STALE_TREE_REFUSAL } from "@exp/domain-contract/sealed-content"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, notInArray, sql } from "drizzle-orm"
import { devices, installs, sessionKeys, sessions, teamMembers, type Session } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"

// 32 bytes as unpadded base64url (43 chars; the last carries 4 bits).
export const PUBLIC_KEY_RE = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/
// push_seal of a 32-byte secret = 81 bytes = exactly 108 base64url chars.
export const WRAPPED_RE = /^[A-Za-z0-9_-]{108}$/

/** Anything that can `select` (the pool, a transaction, the test fake). */
export type Reader = Pick<typeof import("@/db/connection").db, `select`>

export interface TreeReader {
  installId: string
  userId: string
  x25519Public: string
  /** An install of the device executing a session in the tree. */
  executing: boolean
}

export type Root = Pick<Session, `id` | `teamId` | `userId` | `shared` | `parentId` | `sealPublicKey` | `sealEpoch`>

export const notFound = () => new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

/** The readers of `root`'s tree, by install id. */
export async function treeReaders(database: Reader, root: Root): Promise<Map<string, TreeReader>> {
  return (await treeReadersMany(database, [root])).get(root.id) ?? new Map()
}

/** `treeReaders` of many roots at once, in a fixed number of queries
 *  (#PERF-6: `sessionKeys.pending` ran ~6 queries per held root, sequentially,
 *  so a holder of a few hundred trees took seconds). Roots are keyed by id. */
export async function treeReadersMany(database: Reader, roots: readonly Root[]): Promise<Map<string, Map<string, TreeReader>>> {
  const out = new Map<string, Map<string, TreeReader>>()
  for (const root of roots) out.set(root.id, new Map())
  if (roots.length === 0) return out
  const rootIds = [...out.keys()]

  // Team members of every root's team.
  const teamIds = [...new Set(roots.map((root) => root.teamId))]
  const membersOf = new Map<string, Set<string>>()
  for (const teamId of teamIds) membersOf.set(teamId, new Set())
  for (const row of await database
    .select({ teamId: teamMembers.teamId, userId: teamMembers.userId })
    .from(teamMembers)
    .where(inArray(teamMembers.teamId, teamIds))) {
    membersOf.get(row.teamId)?.add(row.userId)
  }

  // The executing devices: the host row of every (machine, starter) in each tree.
  const tree = await database
    .select({ rootId: sessions.rootId, deviceId: sessions.deviceId, userId: sessions.userId, teamId: sessions.teamId })
    .from(sessions)
    .where(inArray(sessions.rootId, rootIds))
  const hostsOf = new Map<string, Map<string, { deviceId: string; userId: string; teamId: string }>>()
  for (const row of tree) {
    if (!row.rootId || !out.has(row.rootId)) continue
    const hosts = hostsOf.get(row.rootId) ?? new Map()
    hosts.set(`${row.deviceId}\n${row.userId}`, row)
    hostsOf.set(row.rootId, hosts)
  }
  const machineIds = [...new Set(tree.filter((row) => row.rootId && out.has(row.rootId)).map((row) => row.deviceId))]
  type DeviceRow = { id: string; userId: string; deviceId: string; sharedTeamIds: string[] | null; createdAt: Date }
  const devicesOf = new Map<string, DeviceRow[]>()
  if (machineIds.length > 0) {
    const rows = await database
      .select({
        id: devices.id,
        userId: devices.userId,
        deviceId: devices.deviceId,
        sharedTeamIds: devices.sharedTeamIds,
        createdAt: devices.createdAt,
      })
      .from(devices)
      .where(inArray(devices.deviceId, machineIds))
    for (const row of rows) devicesOf.set(row.deviceId, [...(devicesOf.get(row.deviceId) ?? []), row])
    // resolveSessionHost's SHARING_ORDER: oldest row first, then id.
    for (const list of devicesOf.values()) {
      list.sort((a, b) => ms(a.createdAt) - ms(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    }
  }
  /** resolveSessionHost over the prefetched rows. */
  const hostOf = (key: { deviceId: string; userId: string; teamId: string }) => {
    const rows = devicesOf.get(key.deviceId) ?? []
    if (rows.some((row) => row.userId === key.userId)) return key.userId
    return rows.find((row) => (row.sharedTeamIds ?? []).includes(key.teamId))?.userId ?? null
  }
  const deviceRowsOf = new Map<string, Set<string>>()
  for (const root of roots) {
    const members = membersOf.get(root.teamId) ?? new Set<string>()
    const rowIds = new Set<string>()
    for (const key of hostsOf.get(root.id)?.values() ?? []) {
      const host = hostOf(key)
      if (!host || !members.has(host)) continue
      for (const row of devicesOf.get(key.deviceId) ?? []) {
        if (row.userId === host && row.deviceId === key.deviceId) rowIds.add(row.id)
      }
    }
    deviceRowsOf.set(root.id, rowIds)
  }

  // Every candidate install in one query: by user (starters / members) or by device row.
  const userIdsOf = new Map<string, string[]>()
  for (const root of roots) {
    const members = membersOf.get(root.teamId) ?? new Set<string>()
    userIdsOf.set(root.id, root.shared ? [...members] : members.has(root.userId) ? [root.userId] : [])
  }
  const allUserIds = [...new Set([...userIdsOf.values()].flat())]
  const allDeviceRowIds = [...new Set([...deviceRowsOf.values()].flatMap((set) => [...set]))]
  const byUser = new Map<string, Array<typeof installs.$inferSelect>>()
  const byDevice = new Map<string, Array<typeof installs.$inferSelect>>()
  if (allUserIds.length > 0) {
    for (const row of await database.select().from(installs).where(inArray(installs.userId, allUserIds))) {
      if (allUserIds.includes(row.userId)) byUser.set(row.userId, [...(byUser.get(row.userId) ?? []), row])
    }
  }
  if (allDeviceRowIds.length > 0) {
    for (const row of await database.select().from(installs).where(inArray(installs.deviceId, allDeviceRowIds))) {
      if (row.deviceId !== null) byDevice.set(row.deviceId, [...(byDevice.get(row.deviceId) ?? []), row])
    }
  }

  for (const root of roots) {
    const members = membersOf.get(root.teamId) ?? new Set<string>()
    const readers = out.get(root.id)!
    const add = (rows: Array<typeof installs.$inferSelect>, executing: boolean) => {
      for (const row of rows) {
        if (row.revokedAt || !members.has(row.userId)) continue
        const known = readers.get(row.id)
        readers.set(row.id, {
          installId: row.id,
          userId: row.userId,
          x25519Public: row.x25519Public,
          executing: executing || (known?.executing ?? false),
        })
      }
    }
    add((userIdsOf.get(root.id) ?? []).flatMap((userId) => byUser.get(userId) ?? []), false)
    add([...(deviceRowsOf.get(root.id) ?? [])].flatMap((rowId) => byDevice.get(rowId) ?? []), true)
  }
  return out
}

const ms = (at: Date | null | undefined) => (at ? at.getTime() : 0)

/** The caller's own, non-revoked install. */
export async function ownInstall(database: Reader, userId: string, installId: string) {
  const [row] = await database
    .select()
    .from(installs)
    .where(and(eq(installs.id, installId), eq(installs.userId, userId)))
    .limit(1)
  if (!row || row.userId !== userId || row.revokedAt) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Not your install` })
  }
  return row
}

export async function loadRoot(database: Reader, rootId: string): Promise<Session> {
  const [root] = await database.select().from(sessions).where(eq(sessions.id, rootId)).limit(1)
  if (!root || root.parentId !== null || root.id !== rootId) throw notFound()
  return root
}

/** The input of a tree key write (`create`, and EXP-1458's inline
 *  `sessions.send {seal}` for a root without a key). */
export interface TreeKeyInput {
  rootId: string
  installId: string
  epoch: number
  publicKey: string
  wraps: Array<{ installId: string; wrapped: string }>
}

/** `sessionKeys.create` inside the caller's transaction: a tree's first key
 *  (epoch 1) or a rotation (`seal_epoch + 1`, by the starter or the
 *  executing device), with wraps for readers including the caller's own. */
export async function createTreeKeyInTx(
  tx: Tx,
  userId: string,
  input: TreeKeyInput
): Promise<{ rootId: string; epoch: number; wraps: number }> {
  if (!PUBLIC_KEY_RE.test(input.publicKey) || !input.wraps.every((wrap) => WRAPPED_RE.test(wrap.wrapped))) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Malformed tree key` })
  }
  await ownInstall(tx, userId, input.installId)
  const targets = input.wraps.map((wrap) => wrap.installId)
  if (new Set(targets).size !== targets.length) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `One wrap per install` })
  }
  if (!targets.includes(input.installId)) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Include a wrap for your own install` })
  }
  // Serializes one tree's key writes (create vs create, rotation vs rotation).
  await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`session_keys:${input.rootId}`}))`)
  const root = await loadRoot(tx, input.rootId)
  const readers = await treeReaders(tx, root)
  const caller = readers.get(input.installId)
  if (!caller) throw notFound()
  if (root.sealEpoch === null) {
    if (input.epoch !== 1) {
      throw new TRPCError({ code: `CONFLICT`, message: `A tree's first key is epoch 1` })
    }
  } else {
    if (input.epoch !== root.sealEpoch + 1) {
      // #FEED-248: a writer whose view of the tree is stale (an inline
      // `seal` for a conversation it did not know was keyed): it resolves
      // the tree again and re-seals (sealed-content.json `staleTree`).
      throw new TRPCError({ code: `CONFLICT`, message: STALE_TREE_REFUSAL })
    }
    if (root.userId !== userId && !caller.executing) {
      throw new TRPCError({
        code: `FORBIDDEN`,
        message: `Only the session's starter or its device rotates the key`,
      })
    }
  }
  for (const target of targets) {
    if (!readers.has(target)) {
      throw new TRPCError({ code: `BAD_REQUEST`, message: `A wrap names an install that is not a reader` })
    }
  }
  for (const wrap of input.wraps) {
    await tx.insert(sessionKeys).values({
      rootId: root.id,
      epoch: input.epoch,
      installId: wrap.installId,
      wrapped: wrap.wrapped,
      createdByInstallId: input.installId,
    })
  }
  await tx
    .update(sessions)
    // A rotation supersedes the epoch a private turn flagged (#FEED-177).
    .set({ sealPublicKey: input.publicKey, sealEpoch: input.epoch, sealRotateEpoch: null })
    .where(eq(sessions.id, root.id))
  return { rootId: root.id, epoch: input.epoch, wraps: input.wraps.length }
}

/**
 * #FEED-177: a tree that turns PRIVATE (its root re-stamped `shared: false`
 * in `tx`, before this runs) keeps only its new readers' wraps: every
 * teammate install's `session_keys` row goes, so `mine` never hands them
 * the key again and `pending` (readers only) never re-wraps it for them.
 * The server cannot rotate (it never sees a secret; a rotation is
 * `create {epoch + 1}` by the starter or the executing device), so what an
 * install already unwrapped stays readable to it until a device rotates:
 * when any wrap went, the root's `seal_rotate_epoch` names its current
 * epoch and the holders (`steer::seal_keys`) rotate; `create` clears it.
 */
export async function dropNonReaderKeysInTx(tx: Tx, rootIds: string[]): Promise<void> {
  for (const rootId of rootIds) {
    // Serializes with the tree's key writes (`createTreeKeyInTx`).
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`session_keys:${rootId}`}))`)
    const [root] = await tx.select().from(sessions).where(eq(sessions.id, rootId)).limit(1)
    if (!root || root.parentId !== null || root.sealEpoch === null) continue
    const readers = [...(await treeReaders(tx, root)).keys()]
    const dropped = await tx
      .delete(sessionKeys)
      .where(
        and(
          eq(sessionKeys.rootId, root.id),
          ...(readers.length > 0 ? [notInArray(sessionKeys.installId, readers)] : [])
        )
      )
      .returning({ installId: sessionKeys.installId })
    if (dropped.length > 0 && root.sealRotateEpoch === null) {
      await tx.update(sessions).set({ sealRotateEpoch: root.sealEpoch }).where(eq(sessions.id, root.id))
    }
  }
}
