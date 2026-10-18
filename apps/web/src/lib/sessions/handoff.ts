import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { SESSION_HANDOFF_MAX_BYTES, type SessionHandoff } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { sessions, type Session } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertDeviceHost, loadSessionForUser, sessionDeviceOwner } from "@/lib/sessions/access"
import { postMovedMessage } from "@/lib/sessions/messages"
import {
  deviceName,
  loadDeviceRow,
  movedText,
  resolveMoveTarget,
} from "@/lib/sessions/move"
import type { Tx } from "@/lib/sessions/tx"
import { getObject, toResponseBody, uploadObject } from "@/lib/storage"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// EXP-1252: the device half of a move. The SOURCE host uploads the handoff
// package (`PUT /api/sessions/{id}/handoff`) and reports the outcome
// (`sessions.handoff`): `moved` re-stamps the row onto the target (queued,
// its messages still pending), `failed` clears the move and the session
// stays put. The TARGET reads the package (`GET`, `sessions.brief.handoff`)
// and calls `sessions.handoffDone` once restored: the blob goes. Packages
// never outlive a week (the session attachment sweep).

export const handoffStorageKey = (sessionId: string) =>
  `session-handoffs/${sessionId}/${crypto.randomUUID()}.tar.gz`

/** What the target's brief says about the package, or null. */
export function briefHandoff(
  session: Pick<Session, `handoff`>
): { sizeBytes: number; fromDeviceId: string; fromDeviceName: string } | null {
  const blob = session.handoff?.blob
  if (!blob) return null
  return {
    sizeBytes: blob.sizeBytes,
    fromDeviceId: blob.fromDeviceId,
    fromDeviceName: blob.fromDeviceName ?? blob.fromDeviceId,
  }
}

async function lockSession(tx: Tx, sessionId: string): Promise<Session> {
  const [row] = await tx.select().from(sessions).where(eq(sessions.id, sessionId)).for(`update`)
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  return row
}

/** The source device's own check: it hosts the row and a move is pending. */
function assertSourceWithMove(session: Session, deviceId: string): void {
  if (deviceId !== session.deviceId) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `This session runs on another device` })
  }
  if (!session.moveToDeviceId || session.status === `ended`) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `No move is pending` })
  }
}

/** Clears a pending move (the session stays where it is) and says why. */
async function failMove(
  sessionId: string,
  reason: string | null
): Promise<{ txid: number; storageKeys: string[] }> {
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const row = await lockSession(tx, sessionId)
    const blob = row.handoff?.blob
    await tx
      .update(sessions)
      .set({ moveToDeviceId: null, handoff: null, caption: null, updatedAt: new Date() })
      .where(eq(sessions.id, sessionId))
    // A daemon may already say "Couldn't move the session: …": never twice.
    const why = reason?.trim().replace(/^Couldn't move the session:?\s*/i, ``) ?? ``
    const text = why ? `Couldn't move the session: ${why.slice(0, 500)}` : `Couldn't move the session`
    await postMovedMessage(tx, row, text)
    return { txid, storageKeys: blob ? [blob.storageKey] : [] }
  })
}

/** `sessions.handoff`: the source's outcome of a pending move. */
export async function completeHandoff(args: {
  userId: string
  sessionId: string
  deviceId: string
  outcome: `moved` | `failed`
  reason?: string | null
}): Promise<{ txid: number }> {
  const session = await loadSessionForUser(args.userId, args.sessionId)
  assertDeviceHost(args.userId, session)
  assertSourceWithMove(session, args.deviceId)

  if (args.outcome === `failed`) {
    const result = await failMove(session.id, args.reason ?? null)
    await deleteStorageObjects(result.storageKeys)
    return { txid: result.txid }
  }

  const targetId = session.moveToDeviceId!
  let target: Awaited<ReturnType<typeof resolveMoveTarget>>
  try {
    // The target may blink offline between the request and now: the row
    // waits for it like any queued session.
    target = await resolveMoveTarget(session, targetId, {
      account: session.handoff?.request?.account ?? null,
      requireOnline: false,
    })
  } catch (error) {
    const reason = error instanceof Error ? error.message : null
    const result = await failMove(session.id, reason)
    await deleteStorageObjects(result.storageKeys)
    throw error
  }
  const from = deviceName(
    await loadDeviceRow(session.deviceId, sessionDeviceOwner(session)),
    session.deviceId
  )

  const txid = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const row = await lockSession(tx, session.id)
    // A cancel or a retarget raced the handoff: the source keeps it.
    if (row.moveToDeviceId !== targetId || row.deviceId !== session.deviceId) {
      throw new TRPCError({ code: `CONFLICT`, message: `The move changed; the session stays here` })
    }
    const handoff: SessionHandoff | null = row.handoff?.blob
      ? { request: null, blob: row.handoff.blob }
      : null
    await tx
      .update(sessions)
      .set({
        deviceId: target.deviceId,
        hostUserId: target.hostUserId,
        account: target.account,
        status: `queued`,
        busy: false,
        needsInput: false,
        blocked: null,
        caption: null,
        moveToDeviceId: null,
        handoff,
        updatedAt: new Date(),
      })
      .where(eq(sessions.id, session.id))
    await postMovedMessage(tx, row, movedText(from, target.name))
    return txid
  })
  const config = getSteerRelayConfig()
  if (config) void relayPostNudge(config, target.ownerId, target.deviceId)
  return { txid }
}

/** `sessions.handoffDone`: the target restored the package; it goes. */
export async function finishHandoff(args: {
  userId: string
  sessionId: string
}): Promise<{ txid: number }> {
  const session = await loadSessionForUser(args.userId, args.sessionId)
  assertDeviceHost(args.userId, session)
  const result = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const row = await lockSession(tx, session.id)
    const blob = row.handoff?.blob
    // A new move requested meanwhile keeps its request.
    const request = row.handoff?.request ?? null
    await tx
      .update(sessions)
      .set({ handoff: request ? { request, blob: null } : null, updatedAt: new Date() })
      .where(eq(sessions.id, session.id))
    return { txid, storageKeys: blob ? [blob.storageKey] : [] }
  })
  await deleteStorageObjects(result.storageKeys)
  return { txid: result.txid }
}

// ── the package (REST: /api/sessions/$sessionId/handoff) ────────────────────

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

const json = (status: number, error: string) => Response.json({ error }, { status })

/** Reads a body up to `max` bytes; null = over the cap. */
async function readCapped(request: Request, max: number): Promise<Uint8Array | null> {
  const reader = request.body?.getReader()
  if (!reader) return new Uint8Array(0)
  const chunks: Uint8Array[] = []
  let total = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    total += value.byteLength
    if (total > max) {
      await reader.cancel().catch(() => {})
      return null
    }
    chunks.push(value)
  }
  const out = new Uint8Array(total)
  let at = 0
  for (const chunk of chunks) {
    out.set(chunk, at)
    at += chunk.byteLength
  }
  return out
}

/** PUT: the SOURCE host uploads the package while a move is pending. */
export async function putHandoffBlob(
  userId: string,
  sessionId: string,
  request: Request,
  maxBytes: number = SESSION_HANDOFF_MAX_BYTES
): Promise<Response> {
  if (!UUID_RE.test(sessionId)) return json(404, `Session not found`)
  const session = await loadSessionForUser(userId, sessionId)
  assertDeviceHost(userId, session)
  if (!session.moveToDeviceId || session.status === `ended`) {
    return json(412, `No move is pending`)
  }
  const declared = Number(request.headers.get(`content-length`) ?? NaN)
  if (Number.isFinite(declared) && declared > maxBytes) {
    return json(413, `The handoff package is over ${maxBytes} bytes`)
  }
  const body = await readCapped(request, maxBytes)
  if (!body) return json(413, `The handoff package is over ${maxBytes} bytes`)
  if (body.byteLength === 0) return json(400, `The handoff package is empty`)

  const storageKey = handoffStorageKey(session.id)
  await uploadObject({
    key: storageKey,
    body,
    contentLength: body.byteLength,
    contentType: `application/gzip`,
  })
  const fromDeviceName = deviceName(
    await loadDeviceRow(session.deviceId, sessionDeviceOwner(session)),
    session.deviceId
  )
  let stale: string[] = []
  try {
    stale = await db.transaction(async (tx) => {
      const row = await lockSession(tx, session.id)
      if (!row.moveToDeviceId || row.deviceId !== session.deviceId) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `No move is pending` })
      }
      const previous = row.handoff?.blob?.storageKey
      await tx
        .update(sessions)
        .set({
          handoff: {
            request: row.handoff?.request ?? null,
            blob: {
              storageKey,
              sizeBytes: body.byteLength,
              fromDeviceId: row.deviceId,
              fromDeviceName,
              uploadedAt: new Date().toISOString(),
            },
          },
          updatedAt: new Date(),
        })
        .where(eq(sessions.id, session.id))
      return previous ? [previous] : []
    })
  } catch (error) {
    await deleteStorageObjects([storageKey])
    if (error instanceof TRPCError && error.code === `PRECONDITION_FAILED`) {
      return json(412, error.message)
    }
    throw error
  }
  await deleteStorageObjects(stale)
  return Response.json({ sizeBytes: body.byteLength }, { status: 201 })
}

/** GET: the session's CURRENT host (the target, after the move) reads it. */
export async function getHandoffBlob(userId: string, sessionId: string): Promise<Response> {
  if (!UUID_RE.test(sessionId)) return json(404, `Session not found`)
  const session = await loadSessionForUser(userId, sessionId)
  assertDeviceHost(userId, session)
  const blob = session.handoff?.blob
  if (!blob) return json(404, `No handoff package`)
  const object = await getObject(blob.storageKey)
  const body = object ? await toResponseBody(object.Body) : null
  if (!body) return json(404, `No handoff package`)
  return new Response(body, {
    status: 200,
    headers: {
      "content-type": `application/gzip`,
      ...(object?.ContentLength ? { "content-length": String(object.ContentLength) } : {}),
      "cache-control": `private, no-store`,
    },
  })
}
