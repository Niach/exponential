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
import { and, eq, inArray } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { devices, installs, sessionKeys, sessions, teamMembers, type Session } from "@/db/schema"
import { nudgeDevice } from "@/lib/trpc/devices"
import { sessionVisibleTo } from "@/lib/sessions/access"
import { sessionDeviceOwner } from "@/lib/sessions/host"
import { isUniqueViolation } from "@/lib/trpc/db-errors"
import {
  createTreeKeyInTx,
  loadRoot,
  ownInstall,
  PUBLIC_KEY_RE,
  treeReaders,
  treeReadersMany,
  WRAPPED_RE,
  type Reader,
} from "@/lib/sessions/tree-keys"

export { createTreeKeyInTx, treeReaders, type TreeKeyInput, type TreeReader } from "@/lib/sessions/tree-keys"

/** The `int` columns' ceiling (contract: epochs are 1…2147483647). */
const EPOCH_MAX = 2_147_483_647
/** `pending` returns at most this many pairs per call. */
export const SESSION_KEYS_PENDING_MAX = 200
const BATCH_MAX = 500
/** `pending` resolves readers this many trees at a time. */
const PENDING_CHUNK = 100


const uuid = z.string().uuid()
const epoch = z.number().int().min(1).max(EPOCH_MAX)
const wrapped = z.string().regex(WRAPPED_RE, `base64url of a wrapped 32-byte secret`)

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

/** The install ids holding a wrap for each root's CURRENT epoch, one query. */
async function wrappedInstallsMany(database: Reader, roots: readonly Session[]): Promise<Map<string, Set<string>>> {
  const epochOf = new Map(roots.map((root) => [root.id, root.sealEpoch]))
  const out = new Map<string, Set<string>>()
  if (roots.length === 0) return out
  const rows = await database
    .select({ rootId: sessionKeys.rootId, epoch: sessionKeys.epoch, installId: sessionKeys.installId })
    .from(sessionKeys)
    .where(inArray(sessionKeys.rootId, [...epochOf.keys()]))
  for (const row of rows) {
    if (!epochOf.has(row.rootId) || epochOf.get(row.rootId) !== row.epoch) continue
    out.set(row.rootId, (out.get(row.rootId) ?? new Set()).add(row.installId))
  }
  return out
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

/** EXP-1531: the fewest ms between two holder nudges for one (install, root). */
export const MISSING_WRAP_NUDGE_MS = 10_000
const lastMissingNudge = new Map<string, number>()

/** EXP-1531: `installId` asked for roots whose CURRENT epoch it holds no
 *  wrap of (a reader hit noKey): wake those trees' holder devices now so
 *  the wrap lands in seconds instead of on their next 60 s pass. Only
 *  roots the install is a reader of; throttled per (install, root). */
export async function nudgeMissingWraps(
  database: typeof import("@/db/connection").db,
  installId: string,
  rootIds: string[],
  held: { rootId: string; epoch: number }[],
  now = Date.now()
): Promise<string[]> {
  const roots = (await database.select().from(sessions).where(inArray(sessions.id, rootIds))).filter(
    (root) =>
      rootIds.includes(root.id) &&
      root.parentId === null &&
      root.sealEpoch !== null &&
      !held.some((row) => row.rootId === root.id && row.epoch === root.sealEpoch)
  )
  const nudged: string[] = []
  for (const root of roots) {
    const key = `${installId}:${root.id}`
    if (now - (lastMissingNudge.get(key) ?? 0) < MISSING_WRAP_NUDGE_MS) continue
    if (!(await treeReaders(database, root)).has(installId)) continue
    lastMissingNudge.set(key, now)
    if (lastMissingNudge.size > 10_000) lastMissingNudge.clear()
    const holderInstalls = (
      await database
        .select({ rootId: sessionKeys.rootId, epoch: sessionKeys.epoch, installId: sessionKeys.installId })
        .from(sessionKeys)
        .where(and(eq(sessionKeys.rootId, root.id), eq(sessionKeys.epoch, root.sealEpoch!)))
    )
      .filter((row) => row.rootId === root.id && row.epoch === root.sealEpoch)
      .map((row) => row.installId)
    if (holderInstalls.length === 0) continue
    const deviceRowIds = (await database.select().from(installs).where(inArray(installs.id, holderInstalls)))
      .filter((row) => holderInstalls.includes(row.id) && row.revokedAt === null && row.deviceId)
      .map((row) => row.deviceId!)
    if (deviceRowIds.length > 0) {
      const holders = (await database.select().from(devices).where(inArray(devices.id, deviceRowIds))).filter((row) =>
        deviceRowIds.includes(row.id)
      )
      for (const holder of holders) nudgeDevice(holder.userId, holder.deviceId)
    }
    nudged.push(root.id)
  }
  return nudged
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
    .mutation(({ ctx, input }) =>
      ctx.db.transaction(async (tx) => {
        const created = await createTreeKeyInTx(tx, ctx.session.user.id, input)
        const txid = await generateTxId(tx)
        return { ...created, txid }
      })
    ),

  /** The wraps this install holds (every epoch: old ciphertext stays readable). */
  mine: authedProcedure
    .input(z.object({ installId: uuid, rootIds: z.array(uuid).max(BATCH_MAX).optional() }))
    .query(async ({ ctx, input }) => {
      await ownInstall(ctx.db, ctx.session.user.id, input.installId)
      if (input.rootIds?.length === 0) return []
      const rows = await heldEpochs(ctx.db, input.installId, input.rootIds)
      if (input.rootIds) void nudgeMissingWraps(ctx.db, input.installId, input.rootIds, rows).catch(() => {})
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
      // #PERF-6: only the trees whose CURRENT epoch this install holds, their
      // readers and wraps fetched per chunk in a fixed number of queries.
      const roots = (await ctx.db.select().from(sessions).where(inArray(sessions.id, rootIds)))
        .filter(
          (root) =>
            rootIds.includes(root.id) &&
            root.parentId === null &&
            root.sealEpoch !== null &&
            !!root.sealPublicKey &&
            held.some((row) => row.rootId === root.id && row.epoch === root.sealEpoch)
        )
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
      const out: PendingPair[] = []
      for (let at = 0; at < roots.length && out.length < input.limit; at += PENDING_CHUNK) {
        const chunk = roots.slice(at, at + PENDING_CHUNK)
        const readersOf = await treeReadersMany(ctx.db, chunk)
        const doneOf = await wrappedInstallsMany(ctx.db, chunk)
        for (const root of chunk) {
          const readers = readersOf.get(root.id)
          if (!readers?.has(input.installId)) continue
          const done = doneOf.get(root.id) ?? new Set<string>()
          for (const reader of readers.values()) {
            if (done.has(reader.installId)) continue
            out.push({
              rootId: root.id,
              epoch: root.sealEpoch!,
              publicKey: root.sealPublicKey!,
              targetInstallId: reader.installId,
              targetX25519Public: reader.x25519Public,
            })
          }
          if (out.length >= input.limit) break
        }
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
