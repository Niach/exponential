import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, issues } from "@/db/schema"
import { errorToResponse } from "@/lib/http-errors"
import { isUniqueViolation } from "@/lib/trpc/db-errors"
import { assertWithinStorageLimit } from "@/lib/billing"
import { deleteObject, uploadObject } from "@/lib/storage"
import { verifyAttachmentUploadToken } from "@/lib/storage/attachment-upload-token"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  isInlineMediaContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"
import {
  getVideoMetadata,
  isProbeableVideoContentType,
} from "@/lib/storage/video-metadata"

// EXP-929: the signed issue-attachment upload. One of the app's very few
// anonymous endpoints, and deliberately so: the agent that uploads holds a
// signed token, not a session — `curl -sS -T file <uploadUrl>` is the whole
// client. The HMAC token IS the authorization (see
// `lib/storage/attachment-upload-token.ts`): MCP
// `exponential_attachments_upload` (without dataBase64) mints it only after
// resolving the issue and checking grant + membership, and it binds the
// pre-allocated attachment id, the issue/board/team, the uploader, the
// filename and the content type for ten minutes. Membership is not re-checked
// here — the same reasoning as the EXP-704 download token and the EXP-879
// session-results upload.
//
// The row is INSERTED here, when the bytes land, with the real size and the
// same header probes the inline path runs — never at mint time. An abandoned
// mint therefore leaves no 0-byte phantom in the issue's Files list, and
// `finalizeAttachmentUpload` (EXP-955), which the second MCP call runs, always
// finds the object its row points at. The route ignores the request's own
// Content-Type: the row's type is the one the tool call canonicalized.
//
// Body: PUT (or POST) the raw bytes; a multipart `file` part is accepted too
// so a `curl -F file=@…` line copied from sessions_results still works.

/** The statuses `errorToResponse` has no tRPC code for (411, 413). */
class UploadStatusError extends Error {
  constructor(
    readonly status: number,
    message: string
  ) {
    super(message)
  }
}

// A multipart envelope adds its boundary lines and part headers on top of the
// file; generous, and still a hard bound on what `formData()` may buffer.
const MULTIPART_OVERHEAD_BYTES = 64 * 1024

function alreadyLanded(attachmentId: string) {
  return new TRPCError({
    code: `CONFLICT`,
    message: `This upload already landed. Finalize it with exponential_attachments_upload({ attachmentId: "${attachmentId}" }).`,
  })
}

async function readUploadBody(
  request: Request,
  maxBytes: number,
  tooLarge: string
): Promise<Uint8Array> {
  const requestType = request.headers.get(`content-type`) ?? ``
  const isMultipart = requestType.toLowerCase().startsWith(`multipart/form-data`)
  const declaredHeader = request.headers.get(`content-length`)
  const declared = declaredHeader === null ? Number.NaN : Number(declaredHeader)
  if (isMultipart) {
    // `formData()` buffers the WHOLE body before any part can be measured, so
    // this branch takes no chunked body: a Content-Length within the token's
    // cap comes first (curl -F always sends one).
    if (!Number.isInteger(declared) || declared < 0) {
      throw new UploadStatusError(
        411,
        `A multipart upload needs a Content-Length; send the bytes as the request body instead (curl -T).`
      )
    }
    if (declared > maxBytes + MULTIPART_OVERHEAD_BYTES) {
      throw new UploadStatusError(413, tooLarge)
    }
  } else if (Number.isFinite(declared) && declared > maxBytes) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: tooLarge })
  }
  if (isMultipart) {
    const formData = await request.formData()
    const file = formData.get(`file`)
    if (!(file instanceof File)) {
      throw new TRPCError({
        code: `BAD_REQUEST`,
        message: `Missing file part (send the bytes as the request body, or as a multipart "file" part)`,
      })
    }
    if (file.size > maxBytes) {
      throw new TRPCError({ code: `BAD_REQUEST`, message: tooLarge })
    }
    return new Uint8Array(await file.arrayBuffer())
  }
  // Raw body, streamed with a running total: Content-Length is only the fast
  // reject and chunked bodies carry none.
  const reader = request.body?.getReader()
  if (!reader) return new Uint8Array(0)
  const chunks: Uint8Array[] = []
  let total = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    total += value.byteLength
    if (total > maxBytes) {
      await reader.cancel().catch(() => undefined)
      throw new TRPCError({ code: `BAD_REQUEST`, message: tooLarge })
    }
    chunks.push(value)
  }
  const body = new Uint8Array(total)
  let offset = 0
  for (const chunk of chunks) {
    body.set(chunk, offset)
    offset += chunk.byteLength
  }
  return body
}

async function uploadSignedAttachment({
  params,
  request,
}: {
  params: { token: string }
  request: Request
}) {
  // 401 before ANY database work: a bad or expired token is anonymous, and an
  // anonymous caller learns nothing about which issues exist.
  const payload = verifyAttachmentUploadToken(params.token)
  if (!payload) {
    throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  }

  // A retry after a PUT that did land: the row exists, so say so instead of
  // storing a second copy — the agent's next step is the finalize call.
  const [existing] = await db
    .select({ id: attachments.id })
    .from(attachments)
    .where(eq(attachments.id, payload.a))
    .limit(1)
  if (existing) throw alreadyLanded(payload.a)

  // The issue may have gone (trash purge, delete) inside the token's window;
  // a 404 beats the FK violation the insert would otherwise surface as a 500.
  const [issue] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(eq(issues.id, payload.i))
    .limit(1)
  if (!issue) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
  }

  const contentType = payload.c
  const isImage = isAcceptedImageContentType(contentType)
  const isMedia = isInlineMediaContentType(contentType)
  const body = await readUploadBody(
    request,
    getMaxUploadBytesForContentType(contentType),
    isImage
      ? `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller`
      : `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller`
  )
  if (body.byteLength === 0) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `File is empty` })
  }

  await assertWithinStorageLimit(payload.w, body.byteLength)

  const storageKey = buildAttachmentStorageKey(payload.i, payload.a, payload.f)
  const url = buildAttachmentUrl(payload.a)
  // Same probes as the inline path: images for their pixel size, MP4/MOV
  // media for size + duration. A pdf/zip has neither.
  const media =
    isMedia && isProbeableVideoContentType(contentType)
      ? getVideoMetadata(body)
      : null
  const dimensions = isImage ? getImageDimensions(body) : media

  await uploadObject({
    body,
    contentLength: body.byteLength,
    contentType,
    key: storageKey,
  })

  try {
    await db.insert(attachments).values({
      id: payload.a,
      teamId: payload.w,
      boardId: payload.b,
      issueId: payload.i,
      commentId: payload.m ?? null,
      uploaderId: payload.u,
      filename: payload.f,
      contentType,
      sizeBytes: body.byteLength,
      storageKey,
      url,
      width: dimensions?.width ?? null,
      height: dimensions?.height ?? null,
      durationMs: media?.durationMs ?? null,
    })
  } catch (error) {
    // A duplicate PUT of the same token that raced past the existence check
    // above: the WINNER's row points at this very key (deterministic from the
    // token), so the object is live. Never delete it; same 409 as a retry.
    if (isUniqueViolation(error)) throw alreadyLanded(payload.a)
    // The row never landed, so the object it points at is garbage.
    try {
      await deleteObject(storageKey)
    } catch (deleteError) {
      console.error(`Failed to rollback uploaded attachment object`, deleteError)
    }
    throw error
  }

  return Response.json({
    ok: true,
    id: payload.a,
    url,
    filename: payload.f,
    contentType,
    sizeBytes: body.byteLength,
    width: dimensions?.width ?? null,
    height: dimensions?.height ?? null,
    durationMs: media?.durationMs ?? null,
    next: `exponential_attachments_upload({ attachmentId: "${payload.a}" })`,
  })
}

async function handle(context: { params: { token: string }; request: Request }) {
  try {
    return await uploadSignedAttachment(context)
  } catch (error) {
    if (error instanceof UploadStatusError) {
      return Response.json({ error: error.message }, { status: error.status })
    }
    return errorToResponse(error)
  }
}

export const Route = createFileRoute(`/api/attachment-uploads/$token`)({
  server: {
    handlers: {
      PUT: handle,
      POST: handle,
    },
  },
})
