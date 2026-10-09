import { TRPCError } from "@trpc/server"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, sessionAttachments } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import {
  buildAttachmentUrl,
  buildPendingSessionAttachmentStorageKey,
  buildSessionAttachmentStorageKey,
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

/**
 * Steer attachment upload (EXP-702, wave D any file). Every steered image or
 * file — issue runs included — lands in the session's own server-only store,
 * keeping steering attachments out of the issue's Files section. Same
 * request/response contract and the same per-type caps as the issue `/files`
 * route (images 10 MB, anything else 50 MB), but only the session's OWNER may
 * upload: steering is owner-only (EXP-312), so nobody else can put the
 * resulting link on the wire anyway.
 */
export async function handleSessionAttachmentUpload({
  params,
  request,
}: SessionAttachmentUploadContext) {
  // Same credential surface as the issue upload route: session cookie,
  // bearer and expu_ api keys, with auth-plugin throws downgraded to a
  // clean 401.
  const session = await resolveSession(request)

  if (!session?.user) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: `Unauthorized`,
    })
  }

  // The id column is uuid — reject garbage before Postgres turns it into a
  // 22P02-shaped 500.
  if (!UUID_RE.test(params.sessionId)) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Session not found`,
    })
  }

  const [run] = await db
    .select({
      id: codingSessions.id,
      teamId: codingSessions.teamId,
      userId: codingSessions.userId,
    })
    .from(codingSessions)
    .where(eq(codingSessions.id, params.sessionId))
    .limit(1)

  if (!run) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Session not found`,
    })
  }

  await assertTeamMember(session.user.id, run.teamId)

  if (run.userId !== session.user.id) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the session owner can attach files`,
    })
  }

  return storeSessionImage(request, session.user.id, {
    teamId: run.teamId,
    sessionId: run.id,
  })
}

/**
 * EXP-825: an image or file attached to a START — the Agent page composer,
 * before any session exists. Same rules as the session route (any type, the
 * same per-type caps, the team's storage budget), scoped to the TEAM: the row
 * carries a NULL session_id until the device that runs the start binds it
 * (`codingSessions.start` `attachmentIds`); a start that never happens
 * leaves an orphan the sweep reclaims after its grace window.
 */
export async function handleTeamSessionAttachmentUpload({
  params,
  request,
}: TeamSessionAttachmentUploadContext) {
  const session = await resolveSession(request)

  if (!session?.user) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: `Unauthorized`,
    })
  }

  if (!UUID_RE.test(params.teamId)) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Team not found`,
    })
  }

  await assertTeamMember(session.user.id, params.teamId)

  return storeSessionImage(request, session.user.id, {
    teamId: params.teamId,
    sessionId: null,
  })
}

/**
 * EXP-879: the half of the upload that has no database in it — validate the
 * multipart part, check the team's storage budget, probe the pixels and put
 * the object. Split out of `storeSessionImage` because the session-results
 * route (`/api/session-results/$token`) has to run the DB half inside its own
 * `FOR UPDATE` transaction on the coding_sessions row; the two routes that
 * just insert a row keep using `storeSessionImage`, whose wire contract is
 * unchanged.
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
  scope: { teamId: string; sessionId: string | null },
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
  scope: { teamId: string; sessionId: string | null },
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
  const storageKey =
    scope.sessionId === null
      ? buildPendingSessionAttachmentStorageKey(
          scope.teamId,
          attachmentId,
          filename
        )
      : buildSessionAttachmentStorageKey(
          scope.sessionId,
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

/** The `session_attachments` insert values for a prepared upload. */
export function sessionAttachmentValues(
  prepared: PreparedSessionImage,
  scope: { teamId: string; sessionId: string | null },
  uploaderId: string
) {
  return {
    id: prepared.attachmentId,
    teamId: scope.teamId,
    sessionId: scope.sessionId,
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
  scope: { teamId: string; sessionId: string | null }
) {
  const prepared = await prepareSessionImage(request, scope)

  try {
    await db
      .insert(sessionAttachments)
      .values(sessionAttachmentValues(prepared, scope, uploaderId))
  } catch (error) {
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }

  return sessionImageResponse(prepared)
}
