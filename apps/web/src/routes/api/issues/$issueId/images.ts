import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { errorToResponse } from "@/lib/http-errors"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  isAcceptedImageContentType,
  maxImageUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import { uploadObject, deleteObject } from "@/lib/storage"
import {
  assertTeamMember,
  getIssueTeamContext,
} from "@/lib/team-membership"
import { assertWithinStorageLimit } from "@/lib/billing"

async function uploadIssueImage({
  params,
  request,
}: {
  params: { issueId: string }
  request: Request
}) {
  // Same credential surface as /api/mcp and /api/attachments: MCP clients and
  // api-key holders can upload images too, and auth-plugin throws must become
  // a clean 401 rather than a 500.
  const session = await resolveSession(request)

  if (!session?.user) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: `Unauthorized`,
    })
  }

  const issueContext = await getIssueTeamContext(params.issueId)
  await assertTeamMember(session.user.id, issueContext.teamId)

  const formData = await request.formData()
  const file = formData.get(`file`)

  if (!(file instanceof File)) {
    // Bun's multipart parser mangles a part whose Content-Disposition uses
    // the RFC-legal UNQUOTED token form (`name=file; filename=...` → key
    // "file; filename=", value decoded as a lossy STRING — the bytes are
    // unrecoverable here). Ktor's MultiPartFormDataContent emits exactly
    // that form, so every Android build before the EXP-61 fix lands in this
    // branch. Name the failure instead of a bare "Missing image file".
    const mangled = [...formData.keys()].some((key) => key.startsWith(`file;`))
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: mangled
        ? `Unsupported multipart encoding (unquoted disposition) — update the app`
        : `Missing image file`,
    })
  }

  if (!isAcceptedImageContentType(file.type)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Unsupported image type`,
    })
  }

  if (file.size > maxImageUploadBytes) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Images must be 10 MB or smaller`,
    })
  }

  await assertWithinStorageLimit(issueContext.teamId, file.size)

  // Browser file names arrive verbatim — strip control chars and clamp so the
  // stored display name is always header- and column-safe.
  const filename = sanitizeUploadFilename(file.name, `image`)
  const attachmentId = crypto.randomUUID()
  const storageKey = buildAttachmentStorageKey(
    params.issueId,
    attachmentId,
    filename
  )
  const url = buildAttachmentUrl(attachmentId)
  const body = new Uint8Array(await file.arrayBuffer())
  // Best-effort intrinsic dimensions so clients can pre-size the image; never
  // block the upload if probing fails (e.g. AVIF or a truncated header).
  const dimensions = getImageDimensions(body)

  await uploadObject({
    body,
    contentLength: file.size,
    contentType: file.type,
    key: storageKey,
  })

  try {
    await db.insert(attachments).values({
      id: attachmentId,
      teamId: issueContext.teamId,
      boardId: issueContext.boardId,
      issueId: params.issueId,
      uploaderId: session.user.id,
      filename,
      contentType: file.type,
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
        `Failed to rollback uploaded attachment object`,
        deleteError
      )
    }

    throw error
  }

  return Response.json({
    id: attachmentId,
    url,
    filename,
    contentType: file.type,
    sizeBytes: file.size,
    width: dimensions?.width ?? null,
    height: dimensions?.height ?? null,
  })
}

export const Route = createFileRoute(`/api/issues/$issueId/images`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await uploadIssueImage(context)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
