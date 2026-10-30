// EXP-1458: the core of a tree key write and the readers rule, shared by
// the tRPC `sessionKeys` router and `sessions.send {seal}` (a new root's key
// in its own transaction). Router-free: lib/sessions never imports a router
// (suites mock @/lib/trpc whole).
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, sql } from "drizzle-orm"
import { devices, installs, sessionKeys, sessions, teamMembers, type Session } from "@/db/schema"
import { resolveSessionHost } from "@/lib/sessions/host"
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
  const members = new Set(
    (await database.select({ userId: teamMembers.userId }).from(teamMembers).where(eq(teamMembers.teamId, root.teamId))).map(
      (row) => row.userId
    )
  )
  const out = new Map<string, TreeReader>()
  const add = (rows: Array<typeof installs.$inferSelect>, executing: boolean) => {
    for (const row of rows) {
      if (row.revokedAt || !members.has(row.userId)) continue
      const known = out.get(row.id)
      out.set(row.id, {
        installId: row.id,
        userId: row.userId,
        x25519Public: row.x25519Public,
        executing: executing || (known?.executing ?? false),
      })
    }
  }

  const userIds = root.shared ? [...members] : members.has(root.userId) ? [root.userId] : []
  if (userIds.length > 0) {
    add(await database.select().from(installs).where(inArray(installs.userId, userIds)), false)
  }

  // The executing devices: the host row of every (machine, starter) in the tree.
  const tree = await database
    .select({ deviceId: sessions.deviceId, userId: sessions.userId, teamId: sessions.teamId })
    .from(sessions)
    .where(eq(sessions.rootId, root.id))
  const hosts = new Map<string, { deviceId: string; userId: string; teamId: string }>()
  for (const row of tree) hosts.set(`${row.deviceId}\n${row.userId}`, row)
  const deviceRowIds: string[] = []
  for (const key of hosts.values()) {
    const host = await resolveSessionHost(key, database)
    if (!host || !members.has(host)) continue
    const rows = await database
      .select({ id: devices.id, userId: devices.userId, deviceId: devices.deviceId })
      .from(devices)
      .where(and(eq(devices.userId, host), eq(devices.deviceId, key.deviceId)))
    for (const row of rows) {
      if (row.userId === host && row.deviceId === key.deviceId) deviceRowIds.push(row.id)
    }
  }
  if (deviceRowIds.length > 0) {
    const rows = await database.select().from(installs).where(inArray(installs.deviceId, deviceRowIds))
    add(
      rows.filter((row) => row.deviceId !== null && deviceRowIds.includes(row.deviceId)),
      true
    )
  }
  return out
}

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
      throw new TRPCError({ code: `CONFLICT`, message: `The tree is at epoch ${root.sealEpoch}` })
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
    .set({ sealPublicKey: input.publicKey, sealEpoch: input.epoch })
    .where(eq(sessions.id, root.id))
  return { rootId: root.id, epoch: input.epoch, wraps: input.wraps.length }
}
