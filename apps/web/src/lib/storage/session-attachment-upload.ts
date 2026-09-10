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
  isAcceptedImageContentType,
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
 * Steer-image upload (EXP-702). Every steered image — issue runs included —
 * lands in the session's own server-only store, keeping steering screenshots
 * out of the issue's Files section. Same request/response contract as the
 * issue `/files` route, but deliberately narrower: images only (the steer
 * composer sends nothing else), and only the session's OWNER may upload —
 * steering is owner-only (EXP-312), so nobody else can put the resulting
 * embed on the wire anyway.
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
      message: `Only the session owner can attach images`,
    })
  }

  return storeSessionImage(request, session.user.id, {
    teamId: run.teamId,
    sessionId: run.id,
  })
}

/**
 * EXP-825: an image attached to a START — the Agent page composer, before
 * any session exists. Same rules as the session route (images only, the
 * same size cap, the team's storage budget), scoped to the TEAM: the row
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

/** The shared tail: validate the part, store the object, insert the row. */
async function storeSessionImage(
  request: Request,
  uploaderId: string,
  scope: { teamId: string; sessionId: string | null }
) {
  const formData = await request.formData()
  const file = formData.get(`file`)

  if (!(file instanceof File)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Missing file`,
    })
  }

  const contentType = canonicalizeContentType(file.type)

  if (!isAcceptedImageContentType(contentType)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Only images can be attached to a session`,
    })
  }

  if (file.size === 0) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `File is empty`,
    })
  }

  if (file.size > maxImageUploadBytes) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller`,
    })
  }

  await assertWithinStorageLimit(scope.teamId, file.size)

  const filename = sanitizeUploadFilename(file.name, `image`)
  const attachmentId = crypto.randomUUID()
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
  const body = new Uint8Array(await file.arrayBuffer())
  // Best-effort intrinsic dimensions; never block the upload if probing
  // fails.
  const dimensions = getImageDimensions(body)

  await uploadObject({
    body,
    contentLength: file.size,
    contentType,
    key: storageKey,
  })

  try {
    await db.insert(sessionAttachments).values({
      id: attachmentId,
      teamId: scope.teamId,
      sessionId: scope.sessionId,
      uploaderId,
      filename,
      contentType,
      sizeBytes: file.size,
      storageKey,
      url,
      width: dimensions?.width ?? null,
      height: dimensions?.height ?? null,
    })
  } catch (error) {
    try {
      await deleteObject(storageKey)
    } catch (deleteError) {
      console.error(
        `Failed to rollback uploaded session attachment object`,
        deleteError
      )
    }

    throw error
  }

  return Response.json({
    id: attachmentId,
    url,
    filename,
    contentType,
    sizeBytes: file.size,
    width: dimensions?.width ?? null,
    height: dimensions?.height ?? null,
  })
}
