// EXP-1458: the core of a tree key write and the readers rule, shared by
// the tRPC `sessionKeys` router and `sessions.send {seal}` (a new root's key
// in its own transaction). Router-free: lib/sessions never imports a router
// (suites mock @/lib/trpc whole).
import { CLEAR_TREE_REFUSAL, STALE_TREE_REFUSAL, WRAP_REFUSAL } from "@exp/domain-contract/sealed-content"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, sql } from "drizzle-orm"
import { devices, installs, sessionKeys, sessions, type Session } from "@/db/schema"
import { teamMemberPairs } from "@/lib/auth/membership"
import type { Tx } from "@/lib/sessions/tx"
import { signedWrapColumns } from "@/lib/sessions/wrap-signatures"

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
  /** #EXP-1950: the install's ed25519 key (`installs.endpoint_id`), the recipient a wrap signature names. */
  endpointId: string
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

  // Team members of every root's team (#EXP-1984: a personal team's owner
  // counts, lib/auth/membership.ts) and of every team a session of the tree
  // runs in (an Assistant's child runs in a team of its scope: its host is
  // a member THERE, eagent tree.json `treeTeams`).
  const tree = await database
    .select({ rootId: sessions.rootId, deviceId: sessions.deviceId, userId: sessions.userId, teamId: sessions.teamId })
    .from(sessions)
    .where(inArray(sessions.rootId, rootIds))
  const teamIds = [...new Set([...roots.map((root) => root.teamId), ...tree.map((row) => row.teamId)])]
  const membersOf = new Map<string, Set<string>>()
  for (const teamId of teamIds) membersOf.set(teamId, new Set())
  for (const row of await teamMemberPairs(database, teamIds)) membersOf.get(row.teamId)?.add(row.userId)

  // The executing devices: the host row of every (machine, starter) in each tree.
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
  /** A host device row's account: its installs read as the executing device. */
  const hostOfRow = new Map<string, string>()
  for (const root of roots) {
    const members = membersOf.get(root.teamId) ?? new Set<string>()
    const rowIds = new Set<string>()
    for (const key of hostsOf.get(root.id)?.values() ?? []) {
      const host = hostOf(key)
      if (!host || !(members.has(host) || membersOf.get(key.teamId)?.has(host))) continue
      for (const row of devicesOf.get(key.deviceId) ?? []) {
        if (row.userId === host && row.deviceId === key.deviceId) {
          rowIds.add(row.id)
          hostOfRow.set(row.id, host)
        }
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
        if (row.revokedAt) continue
        // A host's install reads while its host is a member where it runs.
        if (executing ? hostOfRow.get(row.deviceId ?? ``) !== row.userId : !members.has(row.userId)) continue
        const known = readers.get(row.id)
        readers.set(row.id, {
          installId: row.id,
          userId: row.userId,
          x25519Public: row.x25519Public,
          endpointId: row.endpointId,
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

/** Serializes one tree's key writes (create vs create, rotation vs
 *  rotation) and whoever must read a root's key before deciding to write. */
export async function lockTreeKeys(tx: Tx, rootId: string): Promise<void> {
  await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`session_keys:${rootId}`}))`)
}

/** The input of a tree key write (`create`, and EXP-1458's inline
 *  `sessions.send {seal}` for a root without a key). */
export interface TreeKeyInput {
  rootId: string
  installId: string
  epoch: number
  publicKey: string
  /** #EXP-1950: `signature` = the caller's install's over the wrap (optional). */
  wraps: Array<{ installId: string; wrapped: string; signature?: string | null }>
}

/** `sessionKeys.create` inside the caller's transaction: a tree's first key
 *  (epoch 1) or a rotation (`seal_epoch + 1`, by the starter or the
 *  executing device, or by any reader while the root is due), with wraps
 *  for readers including the caller's own. */
export async function createTreeKeyInTx(
  tx: Tx,
  userId: string,
  input: TreeKeyInput
): Promise<{ rootId: string; epoch: number; wraps: number }> {
  if (!PUBLIC_KEY_RE.test(input.publicKey) || !input.wraps.every((wrap) => WRAPPED_RE.test(wrap.wrapped))) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Malformed tree key` })
  }
  const signer = await ownInstall(tx, userId, input.installId)
  const targets = input.wraps.map((wrap) => wrap.installId)
  if (new Set(targets).size !== targets.length) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `One wrap per install` })
  }
  if (!targets.includes(input.installId)) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Include a wrap for your own install` })
  }
  await lockTreeKeys(tx, input.rootId)
  const root = await loadRoot(tx, input.rootId)
  // #EDE-97: an outside MCP client's tree stays in clear for life.
  if (root.clearTree) throw new TRPCError({ code: `BAD_REQUEST`, message: CLEAR_TREE_REFUSAL })
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
    // #EXP-1951 (`rotation.rotatorCases`): a root that is DUE (a reader
    // was dropped) is rotated by the next reader that is online.
    if (root.userId !== userId && !caller.executing && root.sealRotateEpoch == null) {
      throw new TRPCError({
        code: `FORBIDDEN`,
        message: `Only the session's starter or its device rotates the key`,
      })
    }
  }
  for (const target of targets) {
    if (!readers.has(target)) {
      throw new TRPCError({ code: `BAD_REQUEST`, message: WRAP_REFUSAL })
    }
  }
  // #EXP-1950: a signed wrap is the caller's install's over exactly this row.
  const signed = signedWrapColumns(
    signer.endpointId,
    { rootId: root.id, epoch: input.epoch, treePublicKey: input.publicKey },
    input.wraps.map((wrap) => ({ ...wrap, recipientEndpointId: readers.get(wrap.installId)!.endpointId }))
  )
  for (const [at, wrap] of input.wraps.entries()) {
    await tx.insert(sessionKeys).values({
      rootId: root.id,
      epoch: input.epoch,
      installId: wrap.installId,
      wrapped: wrap.wrapped,
      createdByInstallId: input.installId,
      ...signed[at],
    })
  }
  // #EXP-2119 (`rotation` OLDER EPOCHS): the superseded epoch keeps its
  // public key on its wraps, so a holder can still hand it to a new install.
  if (root.sealEpoch !== null) {
    await tx
      .update(sessionKeys)
      .set({ treePublicKey: root.sealPublicKey })
      .where(and(eq(sessionKeys.rootId, root.id), eq(sessionKeys.epoch, root.sealEpoch)))
  }
  await tx
    .update(sessions)
    // A rotation supersedes the epoch a dropped reader flagged (#FEED-177, #EXP-1951).
    .set({ sealPublicKey: input.publicKey, sealEpoch: input.epoch, sealRotateEpoch: null })
    .where(eq(sessions.id, root.id))
  return { rootId: root.id, epoch: input.epoch, wraps: input.wraps.length }
}

/** `lockTreeKeys` for many roots in ONE statement, in the order given (the
 *  callers sort, so two of them never wait on each other). */
async function lockTreeKeysMany(tx: Tx, rootIds: readonly string[]): Promise<void> {
  if (rootIds.length === 0) return
  await tx.execute(
    sql`SELECT pg_advisory_xact_lock(hashtext('session_keys:' || t.id)) FROM unnest(${sql.param([...rootIds])}::text[]) WITH ORDINALITY AS t(id, n) ORDER BY t.n`
  )
}

/** Roots handled per round of `dropNonReaderKeysInTx` (a fixed number of
 *  queries each, whatever the trees' size). */
export const DROP_CHUNK = 500
const DELETE_CHUNK = 5000

/**
 * A tree's key when a READER IS DROPPED (sealed-content.json `rotation`;
 * #FEED-177: a tree turned private, #EXP-1951: a revoked install, a member
 * removed, an account deleted, a device unshared or removed). Run in `tx`
 * AFTER the write that dropped them: every `session_keys` row of `rootIds`
 * whose install is no longer a reader goes (and every row of `gone`, the
 * installs of an account about to be deleted), so `mine` never hands them
 * the key again and `pending` (readers only) never re-wraps it for them.
 * The server cannot rotate (it never sees a secret; a rotation is
 * `create {epoch + 1}` by a holder), so what an install already unwrapped
 * stays readable to it until a holder rotates: each root that lost a wrap
 * gets `seal_rotate_epoch` = its current epoch (DUE) and the holders rotate
 * (`steer::seal_keys`, the web / iOS / Android key holders through
 * `sessionKeys.due`); `create` clears it. Returns the roots it flagged.
 * A fixed number of queries per DROP_CHUNK roots, never some per root.
 */
export async function dropNonReaderKeysInTx(
  tx: Tx,
  rootIds: readonly string[],
  options: { gone?: ReadonlySet<string> } = {}
): Promise<string[]> {
  const flagged: string[] = []
  const ids = [...new Set(rootIds)].sort()
  for (let at = 0; at < ids.length; at += DROP_CHUNK) {
    const chunk = new Set(ids.slice(at, at + DROP_CHUNK))
    // Serializes with the trees' key writes (`createTreeKeyInTx`, `wrap`).
    await lockTreeKeysMany(tx, [...chunk])
    const roots = (await tx.select().from(sessions).where(inArray(sessions.id, [...chunk]))).filter(
      (root) => chunk.has(root.id) && root.parentId === null && root.sealEpoch !== null
    )
    if (roots.length === 0) continue
    const keyed = new Set(roots.map((root) => root.id))
    const readersOf = await treeReadersMany(tx, roots)
    const held = await tx
      .select({ id: sessionKeys.id, rootId: sessionKeys.rootId, installId: sessionKeys.installId })
      .from(sessionKeys)
      .where(inArray(sessionKeys.rootId, [...keyed]))
    const dropped = held.filter(
      (row) => keyed.has(row.rootId) && (options.gone?.has(row.installId) || !readersOf.get(row.rootId)?.has(row.installId))
    )
    if (dropped.length === 0) continue
    for (let from = 0; from < dropped.length; from += DELETE_CHUNK) {
      await tx.delete(sessionKeys).where(
        inArray(
          sessionKeys.id,
          dropped.slice(from, from + DELETE_CHUNK).map((row) => row.id)
        )
      )
    }
    const lost = new Set(dropped.map((row) => row.rootId))
    const due = roots.filter((root) => lost.has(root.id) && root.sealRotateEpoch === null)
    // One write per epoch among them (nearly always one).
    for (const epoch of new Set(due.map((root) => root.sealEpoch!))) {
      await tx
        .update(sessions)
        .set({ sealRotateEpoch: epoch })
        .where(
          inArray(
            sessions.id,
            due.filter((root) => root.sealEpoch === epoch).map((root) => root.id)
          )
        )
    }
    flagged.push(...due.map((root) => root.id))
  }
  return flagged
}

/** Whose wraps: these installs, an account's installs, or the installs
 *  bound to these devices rows. */
export type KeyHolders = { installIds: readonly string[] } | { userId: string } | { deviceRowIds: readonly string[] }

/** The roots in which `who` holds a wrap (an install without one never had
 *  the tree's secret), in a fixed number of queries; `teamIds` = only the trees with a
 *  session in one of these teams. */
export async function rootsHeldBy(tx: Tx, who: KeyHolders, teamIds?: readonly string[]): Promise<string[]> {
  if (teamIds?.length === 0) return []
  let installIds: string[]
  if (`installIds` in who) installIds = [...who.installIds]
  else if (`userId` in who) {
    const rows = await tx.select({ id: installs.id, userId: installs.userId }).from(installs).where(eq(installs.userId, who.userId))
    installIds = rows.filter((row) => row.userId === who.userId).map((row) => row.id)
  } else {
    if (who.deviceRowIds.length === 0) return []
    const rows = await tx
      .select({ id: installs.id, deviceId: installs.deviceId })
      .from(installs)
      .where(inArray(installs.deviceId, [...who.deviceRowIds]))
    installIds = rows.filter((row) => row.deviceId !== null && who.deviceRowIds.includes(row.deviceId)).map((row) => row.id)
  }
  if (installIds.length === 0) return []
  const held = await tx
    .select({ rootId: sessionKeys.rootId, installId: sessionKeys.installId })
    .from(sessionKeys)
    .where(inArray(sessionKeys.installId, installIds))
  const rootIds = [...new Set(held.filter((row) => installIds.includes(row.installId)).map((row) => row.rootId))]
  if (!teamIds) return rootIds
  const inTeams = new Set<string>()
  for (let at = 0; at < rootIds.length; at += DELETE_CHUNK) {
    const chunk = rootIds.slice(at, at + DELETE_CHUNK)
    const rows = await tx
      .select({ rootId: sessions.rootId, teamId: sessions.teamId })
      .from(sessions)
      .where(and(inArray(sessions.rootId, chunk), inArray(sessions.teamId, [...teamIds])))
    for (const row of rows) if (row.rootId && chunk.includes(row.rootId) && teamIds.includes(row.teamId)) inTeams.add(row.rootId)
  }
  return [...inTeams]
}

/** #EXP-1951: `who` stopped reading (the write that dropped them ran in
 *  `tx`): their wraps go and the trees are due. */
export async function dropReaderKeysInTx(tx: Tx, who: KeyHolders, teamIds?: readonly string[]): Promise<string[]> {
  return dropNonReaderKeysInTx(tx, await rootsHeldBy(tx, who, teamIds))
}

/** #EXP-1951: an account about to be DELETED in `tx` (its installs and
 *  their wraps cascade away with it): the wraps go now and every tree they
 *  read is due. */
export async function dropAccountKeysInTx(tx: Tx, userId: string): Promise<string[]> {
  const rootIds = await rootsHeldBy(tx, { userId })
  if (rootIds.length === 0) return []
  const own = await tx.select({ id: installs.id, userId: installs.userId }).from(installs).where(eq(installs.userId, userId))
  return dropNonReaderKeysInTx(tx, rootIds, { gone: new Set(own.filter((row) => row.userId === userId).map((row) => row.id)) })
}
