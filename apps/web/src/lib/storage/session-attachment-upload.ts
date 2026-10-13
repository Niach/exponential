import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import {
  buildAttachmentUrl,
  buildPendingSessionAttachmentStorageKey,
  canonicalizeContentType,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import { uploadObject, deleteObject } from "@/lib/storage"
import { assertTeamMember } from "@/lib/team-membership"
import { assertWithinStorageLimit } from "@/lib/billing"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"

// EXP-1291: conversation + Guide uploads. Every file a person (or an agent)
// attaches to a `sessions.send` message, and every Guide picture, is a
// team-scoped `attachments` row with NO owner yet (issue, draft and message
// all NULL): `sessions.send({attachmentIds})` claims a message's onto it
// (`message_id`), a Guide entry references a picture by id, and the orphan
// sweep (lib/session-attachment-sweep.ts) reclaims the rest after its grace
// window. Never synced; bytes via /api/attachments/{id}.

export interface SessionAttachmentUploadContext {
  params: { sessionId: string }
  request: Request
}

export interface TeamSessionAttachmentUploadContext {
  params: { teamId: string }
  request: Request
}

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** An upload for a message INTO a session: whoever may post there. */
export async function handleSessionAttachmentUpload({
  params,
  request,
}: SessionAttachmentUploadContext) {
  const session = await resolveSession(request)
  if (!session?.user) {
    throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  }
  if (!UUID_RE.test(params.sessionId)) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  }
  const target = await loadSessionForUser(session.user.id, params.sessionId)
  await assertCanSteer(session.user.id, target)
  return storeSessionImage(request, session.user.id, { teamId: target.teamId })
}

/** An upload for a START (the composer, before any session exists): any
 *  team member. */
export async function handleTeamSessionAttachmentUpload({
  params,
  request,
}: TeamSessionAttachmentUploadContext) {
  const session = await resolveSession(request)
  if (!session?.user) {
    throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  }
  if (!UUID_RE.test(params.teamId)) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Team not found` })
  }
  await assertTeamMember(session.user.id, params.teamId)
  return storeSessionImage(request, session.user.id, { teamId: params.teamId })
}

/**
 * The half of the upload with no database in it — validate the multipart
 * part, check the team's storage budget, probe the pixels and put the
 * object. The Guide upload route runs the DB half inside its own
 * `FOR UPDATE` transaction on the thread's root.
 */
export interface PreparedSessionImage {
  attachmentId: string
  filename: string
  contentType: string
  sizeBytes: number
  storageKey: string
  url: string
  width: number | null
  height: number | null
}

export interface PrepareSessionAttachmentOptions {
  /** Result pictures (`sessions_show`, `/api/session-results`) stay images;
   *  the steer and start uploads take any file. */
  imagesOnly?: boolean
}

export async function prepareSessionImage(
  request: Request,
  scope: { teamId: string; sessionId?: string | null },
  /** EXP-1172: a pre-allocated id (a sessions_show grant); else a fresh one. */
  attachmentId?: string,
  options: PrepareSessionAttachmentOptions = {}
): Promise<PreparedSessionImage> {
  const formData = await request.formData()
  const file = formData.get(`file`)

  if (!(file instanceof File)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Missing file`,
    })
  }

  return prepareSessionImageBytes(
    {
      filename: file.name,
      contentType: file.type,
      body: new Uint8Array(await file.arrayBuffer()),
      size: file.size,
    },
    scope,
    attachmentId,
    options
  )
}

/** EXP-1172: the same validate-store-probe half for bytes already in hand —
 *  the multipart route above and `exponential_sessions_show`'s `dataBase64`. */
export async function prepareSessionImageBytes(
  file: {
    filename: string
    contentType: string
    body: Uint8Array
    /** The part's declared size; the bytes' length without one. */
    size?: number
  },
  scope: { teamId: string; sessionId?: string | null },
  attachmentId: string = crypto.randomUUID(),
  options: PrepareSessionAttachmentOptions = {}
): Promise<PreparedSessionImage> {
  const contentType = canonicalizeContentType(file.contentType)
  const isImage = isAcceptedImageContentType(contentType)

  if (options.imagesOnly && !isImage) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Only images can be attached to a session`,
    })
  }

  const size = file.size ?? file.body.byteLength
  if (size === 0) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `File is empty`,
    })
  }

  if (size > getMaxUploadBytesForContentType(contentType)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: isImage
        ? `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller`
        : `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller`,
    })
  }

  await assertWithinStorageLimit(scope.teamId, size)

  const filename = sanitizeUploadFilename(
    file.filename,
    isImage ? `image` : `file`
  )
  const storageKey = buildPendingSessionAttachmentStorageKey(
    scope.teamId,
    attachmentId,
    filename
  )
  const url = buildAttachmentUrl(attachmentId)
  const body = file.body
  // Best-effort intrinsic dimensions, images only; never block the upload if
  // probing fails.
  const dimensions = isImage ? getImageDimensions(body) : null

  await uploadObject({
    body,
    contentLength: size,
    contentType,
    key: storageKey,
  })

  return {
    attachmentId,
    filename,
    contentType,
    sizeBytes: size,
    storageKey,
    url,
    width: dimensions?.width ?? null,
    height: dimensions?.height ?? null,
  }
}

/** The owner-less `attachments` insert values for a prepared upload. */
export function sessionAttachmentValues(
  prepared: PreparedSessionImage,
  scope: { teamId: string },
  uploaderId: string
): typeof attachments.$inferInsert {
  return {
    id: prepared.attachmentId,
    teamId: scope.teamId,
    uploaderId,
    filename: prepared.filename,
    contentType: prepared.contentType,
    sizeBytes: prepared.sizeBytes,
    storageKey: prepared.storageKey,
    url: prepared.url,
    width: prepared.width,
    height: prepared.height,
  }
}

/** Best-effort object reclaim when the row never landed. Never throws — the
 *  caller is already on its way out with the real error. */
export async function rollbackSessionImage(storageKey: string) {
  try {
    await deleteObject(storageKey)
  } catch (deleteError) {
    console.error(
      `Failed to rollback uploaded session attachment object`,
      deleteError
    )
  }
}

/** The wire contract both `/files` routes answer with — byte-identical since
 *  EXP-702, so clients keep reading `id`/`url`/`width`/`height`. */
export function sessionImageResponse(prepared: PreparedSessionImage) {
  return Response.json({
    id: prepared.attachmentId,
    url: prepared.url,
    filename: prepared.filename,
    contentType: prepared.contentType,
    sizeBytes: prepared.sizeBytes,
    width: prepared.width,
    height: prepared.height,
  })
}

/** The shared tail: validate the part, store the object, insert the row. */
async function storeSessionImage(
  request: Request,
  uploaderId: string,
  scope: { teamId: string; sessionId?: string | null }
) {
  const prepared = await prepareSessionImage(request, scope)

  try {
    await db
      .insert(attachments)
      .values(sessionAttachmentValues(prepared, scope, uploaderId))
  } catch (error) {
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }

  return sessionImageResponse(prepared)
}
