// #EXP-1950 (fixtures/wrap-signature.json): the server half of signed tree
// key wraps. A wrap MAY carry its maker's signature; the maker is always the
// caller's own install, so the server verifies it against that install's
// endpoint id over the bytes it is about to store (its root, epoch, tree
// public key and the recipient's endpoint id) and refuses a bad one. That is
// a bug catcher, not the protection: an install keeps a wrap only when an
// install it PINNED signed it (`acceptWrap`, on the clients). A wrap without
// a signature is stored unsigned, as every released client writes it.
import { resignOutcome, verifyWrapSignature } from "@exp/domain-contract/wrap-signature"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNotNull, isNull } from "drizzle-orm"
import { z } from "zod"
import { installs, sessionKeys, sessions } from "@/db/schema"
import { loadRoot, lockTreeKeys, ownInstall, treeReaders, type Reader } from "@/lib/sessions/tree-keys"
import type { Tx } from "@/lib/sessions/tx"

export const WRAP_SIGNATURE_REFUSAL = `A wrap's signature is not your install's over this key`

/** The optional per-wrap input (86 chars when valid; anything else is refused by the check). */
export const wrapSignatureInput = z.string().min(1).max(128).nullish()

export interface SignedColumns {
  signature: string | null
  signerEndpointId: string | null
}

const UNSIGNED: SignedColumns = { signature: null, signerEndpointId: null }

/** The columns of each wrap of one (root, epoch, key), in order: a signed
 *  wrap must be `signerEndpointId`'s signature over exactly what is stored. */
export function signedWrapColumns(
  signerEndpointId: string,
  key: { rootId: string; epoch: number; treePublicKey: string },
  wraps: ReadonlyArray<{ recipientEndpointId: string | null; wrapped: string; signature?: string | null }>
): SignedColumns[] {
  return wraps.map((wrap) => {
    if (wrap.signature == null) return UNSIGNED
    const ok =
      wrap.recipientEndpointId !== null &&
      verifyWrapSignature({ ...key, recipientEndpointId: wrap.recipientEndpointId, wrapped: wrap.wrapped }, wrap.signature, signerEndpointId)
    if (!ok) throw new TRPCError({ code: `BAD_REQUEST`, message: WRAP_SIGNATURE_REFUSAL })
    return { signature: wrap.signature, signerEndpointId }
  })
}

/** `sessionKeys.wrap`: the columns of its signed wraps by `root\nepoch\ntarget`.
 *  Only wraps of a root's CURRENT epoch are checked here (the only ones
 *  `pending` lists); any other pair is refused by the pending rule. */
export async function signedPendingWraps(
  database: Reader,
  signer: { endpointId: string },
  wraps: ReadonlyArray<{ rootId: string; epoch: number; targetInstallId: string; wrapped: string; signature?: string | null }>,
  roots: ReadonlyArray<{ id: string; sealEpoch: number | null; sealPublicKey: string | null }>
): Promise<Map<string, SignedColumns>> {
  const out = new Map<string, SignedColumns>()
  const signed = wraps.filter((wrap) => wrap.signature != null)
  if (signed.length === 0) return out
  const targets = [...new Set(signed.map((wrap) => wrap.targetInstallId))]
  const endpointOf = new Map(
    (await database.select({ id: installs.id, endpointId: installs.endpointId }).from(installs).where(inArray(installs.id, targets)))
      .filter((row) => targets.includes(row.id))
      .map((row) => [row.id, row.endpointId])
  )
  for (const wrap of signed) {
    const root = roots.find((row) => row.id === wrap.rootId)
    if (!root || root.sealEpoch !== wrap.epoch || !root.sealPublicKey || !endpointOf.has(wrap.targetInstallId)) continue
    const [columns] = signedWrapColumns(signer.endpointId, { rootId: root.id, epoch: wrap.epoch, treePublicKey: root.sealPublicKey }, [
      { recipientEndpointId: endpointOf.get(wrap.targetInstallId)!, wrapped: wrap.wrapped, signature: wrap.signature },
    ])
    out.set(`${wrap.rootId}\n${wrap.epoch}\n${wrap.targetInstallId}`, columns!)
  }
  return out
}

/** `sessionKeys.wrap`'s signed wraps, verified against the caller's own
 *  install before anything is written (a bad one refuses the batch); their
 *  columns by `root\nepoch\ntarget`. */
export async function checkWrapSignatures(
  database: Reader,
  userId: string,
  input: { installId: string; wraps: ReadonlyArray<{ rootId: string; epoch: number; targetInstallId: string; wrapped: string; signature?: string | null }> }
): Promise<Map<string, SignedColumns>> {
  if (!input.wraps.some((wrap) => wrap.signature != null)) return new Map()
  const signer = await ownInstall(database, userId, input.installId)
  const rootIds = [...new Set(input.wraps.map((wrap) => wrap.rootId))]
  const roots = (await database.select().from(sessions).where(inArray(sessions.id, rootIds))).filter((root) => rootIds.includes(root.id))
  return signedPendingWraps(database, signer, input.wraps, roots)
}

/** A signed wrap of a root's current epoch made for some install: a client
 *  that holds no secret of that epoch verifies the public key with it
 *  (`acceptVouch`) before it seals. */
export interface Vouch {
  recipientEndpointId: string
  wrapped: string
  signature: string
  signerEndpointId: string
}

/** One vouch per root (its oldest signed wrap of `sealEpoch` whose signer is
 *  a live install), null where none; one query. */
export async function vouchesFor(
  database: Reader,
  roots: ReadonlyArray<{ id: string; sealEpoch: number | null }>
): Promise<Map<string, Vouch>> {
  const out = new Map<string, Vouch>()
  const keyed = roots.filter((root) => root.sealEpoch !== null)
  if (keyed.length === 0) return out
  const epochOf = new Map(keyed.map((root) => [root.id, root.sealEpoch]))
  const rows = await database
    .select({
      rootId: sessionKeys.rootId,
      epoch: sessionKeys.epoch,
      wrapped: sessionKeys.wrapped,
      signature: sessionKeys.signature,
      signerEndpointId: sessionKeys.signerEndpointId,
      recipientEndpointId: installs.endpointId,
    })
    .from(sessionKeys)
    .innerJoin(installs, eq(installs.id, sessionKeys.installId))
    .where(
      and(
        inArray(sessionKeys.rootId, [...epochOf.keys()]),
        isNotNull(sessionKeys.signature),
        isNotNull(sessionKeys.signerEndpointId),
        inArray(
          sessionKeys.signerEndpointId,
          database.select({ endpointId: installs.endpointId }).from(installs).where(isNull(installs.revokedAt))
        )
      )
    )
    .orderBy(asc(sessionKeys.createdAt), asc(sessionKeys.id))
  for (const row of rows) {
    if (out.has(row.rootId) || epochOf.get(row.rootId) !== row.epoch || !row.signature || !row.signerEndpointId) continue
    out.set(row.rootId, {
      recipientEndpointId: row.recipientEndpointId,
      wrapped: row.wrapped,
      signature: row.signature,
      signerEndpointId: row.signerEndpointId,
    })
  }
  return out
}

/** One wrap of `sessionKeys.resign`: `installId` = the TARGET install. */
export interface ResignWrap {
  rootId: string
  epoch: number
  installId: string
  wrapped: string
  signature: string
}

/** `sessionKeys.resign` inside its transaction (wrap-signature.json
 *  `resign`, decision c): the caller, a holder of a root's current epoch,
 *  replaces each reader's UNSIGNED wrap of that epoch with its own signed
 *  one. Per wrap `resignOutcome` decides; one refusal refuses the call
 *  before anything is written. A signed row is never touched. */
export async function resignWrapsInTx(
  tx: Tx,
  userId: string,
  input: { installId: string; wraps: readonly ResignWrap[] }
): Promise<{ replaced: number; skipped: number }> {
  const signer = await ownInstall(tx, userId, input.installId)
  const seen = new Set<string>()
  for (const wrap of input.wraps) {
    const key = `${wrap.rootId}\n${wrap.epoch}\n${wrap.installId}`
    if (seen.has(key)) throw new TRPCError({ code: `BAD_REQUEST`, message: `One wrap per pair` })
    seen.add(key)
  }
  const rootIds = [...new Set(input.wraps.map((wrap) => wrap.rootId))].sort()
  const toReplace: Array<ResignWrap> = []
  let skipped = 0
  for (const rootId of rootIds) {
    await lockTreeKeys(tx, rootId)
    let root: Awaited<ReturnType<typeof loadRoot>>
    try {
      root = await loadRoot(tx, rootId)
    } catch {
      throw new TRPCError({ code: `FORBIDDEN`, message: RESIGN_NOT_READER })
    }
    const readers = await treeReaders(tx, root)
    const wraps = input.wraps.filter((wrap) => wrap.rootId === rootId)
    const current = wraps.filter((wrap) => wrap.epoch === root.sealEpoch)
    const rows = new Map(
      current.length === 0
        ? []
        : (
            await tx
              .select({ installId: sessionKeys.installId, epoch: sessionKeys.epoch, rootId: sessionKeys.rootId, signature: sessionKeys.signature })
              .from(sessionKeys)
              .where(and(eq(sessionKeys.rootId, rootId), eq(sessionKeys.epoch, root.sealEpoch!), inArray(sessionKeys.installId, current.map((wrap) => wrap.installId))))
          )
            .filter((row) => row.rootId === rootId && row.epoch === root.sealEpoch)
            .map((row) => [row.installId, row.signature === null ? (`unsigned` as const) : (`signed` as const)])
    )
    for (const wrap of wraps) {
      const target = readers.get(wrap.installId)
      const currentEpoch = wrap.epoch === root.sealEpoch && !!root.sealPublicKey
      const { rule, outcome } = resignOutcome({
        callerReader: readers.has(input.installId),
        currentEpoch,
        targetReader: !!target,
        signatureValid:
          currentEpoch &&
          !!target &&
          verifyWrapSignature(
            { rootId, epoch: wrap.epoch, treePublicKey: root.sealPublicKey!, recipientEndpointId: target.endpointId, wrapped: wrap.wrapped },
            wrap.signature,
            signer.endpointId
          ),
        row: rows.get(wrap.installId) ?? `none`,
      })
      if (outcome === `refused`) {
        throw rule === `badSignature`
          ? new TRPCError({ code: `BAD_REQUEST`, message: WRAP_SIGNATURE_REFUSAL })
          : new TRPCError({ code: `FORBIDDEN`, message: RESIGN_NOT_READER })
      }
      if (outcome === `replaced`) toReplace.push(wrap)
      else skipped++
    }
  }
  let replaced = 0
  for (const wrap of toReplace) {
    const updated = await tx
      .update(sessionKeys)
      .set({ wrapped: wrap.wrapped, signature: wrap.signature, signerEndpointId: signer.endpointId, createdByInstallId: signer.id })
      .where(
        and(
          eq(sessionKeys.rootId, wrap.rootId),
          eq(sessionKeys.epoch, wrap.epoch),
          eq(sessionKeys.installId, wrap.installId),
          isNull(sessionKeys.signature)
        )
      )
      .returning({ id: sessionKeys.id })
    if (updated.length > 0) replaced++
    else skipped++
  }
  return { replaced, skipped }
}

export const RESIGN_NOT_READER = `Not a reader of this tree`
