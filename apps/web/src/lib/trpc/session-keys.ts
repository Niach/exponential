// EXP-1458: the key API of sealed session content
// (packages/domain-contract/fixtures/sealed-content.json is the spec). Each
// session tree has one X25519 keypair per epoch; its public key sits on the
// ROOT row (`sessions.seal_public_key` / `seal_epoch`, synced) and its
// secret reaches every READER install wrapped (`session_keys`, server-only).
// The server never sees a secret: it only computes who the readers are and
// hands holders the wraps still missing (automatic, no approval).
//
// READERS of a tree (`treeReaders`): the non-revoked installs of
// - the root's starter, while a member of the root's team;
// - every team member, when the root is `shared`;
// - the HOST devices row (lib/sessions/host.ts) of any session in the tree,
//   through `installs.device_id` (the executing device).
// Every procedure names an `installId` the caller owns (not revoked).
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, sql } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { devices, installs, sessionKeys, sessions, teamMembers, type Session } from "@/db/schema"
import { sessionVisibleTo } from "@/lib/sessions/access"
import { resolveSessionHost, sessionDeviceOwner } from "@/lib/sessions/host"
import { isUniqueViolation } from "@/lib/trpc/db-errors"

/** The `int` columns' ceiling (contract: epochs are 1…2147483647). */
const EPOCH_MAX = 2_147_483_647
/** `pending` returns at most this many pairs per call. */
export const SESSION_KEYS_PENDING_MAX = 200
const BATCH_MAX = 500

// 32 bytes as unpadded base64url (43 chars; the last carries 4 bits).
const PUBLIC_KEY_RE = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/
// push_seal of a 32-byte secret = 81 bytes = exactly 108 base64url chars.
const WRAPPED_RE = /^[A-Za-z0-9_-]{108}$/

const uuid = z.string().uuid()
const epoch = z.number().int().min(1).max(EPOCH_MAX)
const wrapped = z.string().regex(WRAPPED_RE, `base64url of a wrapped 32-byte secret`)

/** Anything that can `select` (the pool, a transaction, the test fake). */
type Reader = Pick<typeof import("@/db/connection").db, `select`>

export interface TreeReader {
  installId: string
  userId: string
  x25519Public: string
  /** An install of the device executing a session in the tree. */
  executing: boolean
}

type Root = Pick<Session, `id` | `teamId` | `userId` | `shared` | `parentId` | `sealPublicKey` | `sealEpoch`>

const notFound = () => new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

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
async function ownInstall(database: Reader, userId: string, installId: string) {
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

async function loadRoot(database: Reader, rootId: string): Promise<Session> {
  const [root] = await database.select().from(sessions).where(eq(sessions.id, rootId)).limit(1)
  if (!root || root.parentId !== null || root.id !== rootId) throw notFound()
  return root
}

/** The wraps one install holds: rootId → epochs. */
async function heldEpochs(database: Reader, installId: string, rootIds?: string[]) {
  const where = rootIds
    ? and(eq(sessionKeys.installId, installId), inArray(sessionKeys.rootId, rootIds))
    : eq(sessionKeys.installId, installId)
  return (await database.select().from(sessionKeys).where(where)).filter(
    (row) => row.installId === installId && (!rootIds || rootIds.includes(row.rootId))
  )
}

/** The install ids holding a wrap for (root, epoch). */
async function wrappedInstalls(database: Reader, rootId: string, keyEpoch: number): Promise<Set<string>> {
  const rows = await database
    .select({ rootId: sessionKeys.rootId, epoch: sessionKeys.epoch, installId: sessionKeys.installId })
    .from(sessionKeys)
    .where(and(eq(sessionKeys.rootId, rootId), eq(sessionKeys.epoch, keyEpoch)))
  return new Set(rows.filter((row) => row.rootId === rootId && row.epoch === keyEpoch).map((row) => row.installId))
}

type PendingPair = {
  rootId: string
  epoch: number
  publicKey: string
  targetInstallId: string
  targetX25519Public: string
}

/** What `installId` should wrap on `root`: readers without a wrap for the
 *  current epoch, when `installId` holds that epoch and is still a reader. */
async function pendingFor(database: Reader, root: Session, installId: string, holds: boolean): Promise<PendingPair[]> {
  if (!holds || root.sealEpoch === null || !root.sealPublicKey) return []
  const readers = await treeReaders(database, root)
  if (!readers.has(installId)) return []
  const done = await wrappedInstalls(database, root.id, root.sealEpoch)
  return [...readers.values()]
    .filter((reader) => !done.has(reader.installId))
    .map((reader) => ({
      rootId: root.id,
      epoch: root.sealEpoch!,
      publicKey: root.sealPublicKey!,
      targetInstallId: reader.installId,
      targetX25519Public: reader.x25519Public,
    }))
}

export const sessionKeysRouter = router({
  /** A tree's first key (epoch 1) or a rotation (`seal_epoch + 1`, by the
   *  starter or the executing device), with wraps for readers. */
  create: authedProcedure
    .input(
      z.object({
        rootId: uuid,
        installId: uuid,
        epoch,
        publicKey: z.string().regex(PUBLIC_KEY_RE, `base64url of 32 bytes`),
        wraps: z.array(z.object({ installId: uuid, wrapped })).min(1).max(BATCH_MAX),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await ownInstall(ctx.db, userId, input.installId)
      const targets = input.wraps.map((wrap) => wrap.installId)
      if (new Set(targets).size !== targets.length) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `One wrap per install` })
      }
      if (!targets.includes(input.installId)) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Include a wrap for your own install` })
      }
      return ctx.db.transaction(async (tx) => {
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
        const txid = await generateTxId(tx)
        return { rootId: root.id, epoch: input.epoch, wraps: input.wraps.length, txid }
      })
    }),

  /** The wraps this install holds (every epoch: old ciphertext stays readable). */
  mine: authedProcedure
    .input(z.object({ installId: uuid, rootIds: z.array(uuid).max(BATCH_MAX).optional() }))
    .query(async ({ ctx, input }) => {
      await ownInstall(ctx.db, ctx.session.user.id, input.installId)
      if (input.rootIds?.length === 0) return []
      const rows = await heldEpochs(ctx.db, input.installId, input.rootIds)
      return rows.map((row) => ({ rootId: row.rootId, epoch: row.epoch, wrapped: row.wrapped }))
    }),

  /** Readers still missing the current epoch's wrap, on trees whose current
   *  epoch this install holds. */
  pending: authedProcedure
    .input(
      z.object({
        installId: uuid,
        limit: z.number().int().min(1).max(SESSION_KEYS_PENDING_MAX).default(SESSION_KEYS_PENDING_MAX),
      })
    )
    .query(async ({ ctx, input }) => {
      await ownInstall(ctx.db, ctx.session.user.id, input.installId)
      const held = await heldEpochs(ctx.db, input.installId)
      const rootIds = [...new Set(held.map((row) => row.rootId))].sort()
      if (rootIds.length === 0) return []
      const roots = (await ctx.db.select().from(sessions).where(inArray(sessions.id, rootIds))).filter(
        (root) => rootIds.includes(root.id) && root.parentId === null
      )
      roots.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
      const out: PendingPair[] = []
      for (const root of roots) {
        const holds = held.some((row) => row.rootId === root.id && row.epoch === root.sealEpoch)
        out.push(...(await pendingFor(ctx.db, root, input.installId, holds)))
        if (out.length >= input.limit) break
      }
      return out.slice(0, input.limit)
    }),

  /** Wraps for pairs `pending` lists. A pair another holder wrapped
   *  meanwhile is skipped; any other pair refuses the whole batch. */
  wrap: authedProcedure
    .input(
      z.object({
        installId: uuid,
        wraps: z
          .array(z.object({ rootId: uuid, epoch, targetInstallId: uuid, wrapped }))
          .min(1)
          .max(SESSION_KEYS_PENDING_MAX),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await ownInstall(ctx.db, ctx.session.user.id, input.installId)
      const rootIds = [...new Set(input.wraps.map((wrap) => wrap.rootId))]
      const held = await heldEpochs(ctx.db, input.installId, rootIds)
      const listable = new Set<string>()
      const already = new Set<string>()
      for (const rootId of rootIds) {
        let root: Session
        try {
          root = await loadRoot(ctx.db, rootId)
        } catch {
          throw new TRPCError({ code: `FORBIDDEN`, message: `Not a pending pair` })
        }
        const holds = held.some((row) => row.rootId === root.id && row.epoch === root.sealEpoch)
        for (const pair of await pendingFor(ctx.db, root, input.installId, holds)) {
          listable.add(`${pair.rootId}\n${pair.epoch}\n${pair.targetInstallId}`)
        }
        if (holds && root.sealEpoch !== null) {
          for (const done of await wrappedInstalls(ctx.db, root.id, root.sealEpoch)) {
            already.add(`${root.id}\n${root.sealEpoch}\n${done}`)
          }
        }
      }
      const seen = new Set<string>()
      const toWrite: typeof input.wraps = []
      let skipped = 0
      for (const wrap of input.wraps) {
        const key = `${wrap.rootId}\n${wrap.epoch}\n${wrap.targetInstallId}`
        if (seen.has(key)) {
          throw new TRPCError({ code: `BAD_REQUEST`, message: `One wrap per pair` })
        }
        seen.add(key)
        if (already.has(key)) {
          skipped++
        } else if (listable.has(key)) {
          toWrite.push(wrap)
        } else {
          throw new TRPCError({ code: `FORBIDDEN`, message: `Not a pending pair` })
        }
      }
      let written = 0
      for (const wrap of toWrite) {
        try {
          await ctx.db.insert(sessionKeys).values({
            rootId: wrap.rootId,
            epoch: wrap.epoch,
            installId: wrap.targetInstallId,
            wrapped: wrap.wrapped,
            createdByInstallId: input.installId,
          })
          written++
        } catch (err) {
          // Another holder wrapped the same pair concurrently.
          if (!isUniqueViolation(err)) throw err
          skipped++
        }
      }
      return { written, skipped }
    }),

  /** The current key of each keyed root the caller may read or send to
   *  (keyless and invisible roots are omitted). */
  publicKeys: authedProcedure
    .input(z.object({ rootIds: z.array(uuid).min(1).max(BATCH_MAX) }))
    .query(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const rows = (await ctx.db.select().from(sessions).where(inArray(sessions.id, input.rootIds))).filter(
        (row) => input.rootIds.includes(row.id) && row.parentId === null && row.sealPublicKey && row.sealEpoch !== null
      )
      if (rows.length === 0) return []
      const teams = new Set(
        (
          await ctx.db
            .select({ teamId: teamMembers.teamId, userId: teamMembers.userId })
            .from(teamMembers)
            .where(eq(teamMembers.userId, userId))
        )
          .filter((row) => row.userId === userId)
          .map((row) => row.teamId)
      )
      const out: Array<{ rootId: string; epoch: number; publicKey: string }> = []
      for (const row of rows) {
        const readable = teams.has(row.teamId) && sessionVisibleTo(row, userId)
        // The host of the tree's device acts as its sessions (MCP principal).
        const principal = !readable && (await sessionDeviceOwner(row, ctx.db)) === userId
        if (readable || principal) out.push({ rootId: row.id, epoch: row.sealEpoch!, publicKey: row.sealPublicKey! })
      }
      return out
    }),
})
