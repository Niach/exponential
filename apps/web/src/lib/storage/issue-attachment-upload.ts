import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  canonicalizeContentType,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import { uploadObject, deleteObject } from "@/lib/storage"
import { assertTeamMember, getIssueTeamContext } from "@/lib/team-membership"
import { assertWithinStorageLimit } from "@/lib/billing"

export interface IssueAttachmentUploadContext {
  params: { issueId: string }
  request: Request
}

/**
 * Shared body of both issue upload routes (EXP-297).
 *
 * `imagesOnly: true` is the byte-frozen legacy `/images` contract (5 raster
 * types, 10 MB, "Missing image file" / "Unsupported image type" wording) that
 * old native builds still call. `imagesOnly: false` is `/files`: any content
 * type, 10 MB for inline images and 50 MB for everything else.
 */
export async function handleIssueAttachmentUpload(
  { params, request }: IssueAttachmentUploadContext,
  { imagesOnly }: { imagesOnly: boolean }
) {
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
        ? `Unsupported multipart encoding (unquoted disposition). Update the app.`
        : imagesOnly
          ? `Missing image file`
          : `Missing file`,
    })
  }

  // Canonicalized before any classification: picker-/provider-supplied types
  // are not guaranteed lowercase or parameter-free, and the exact-match
  // inline-image rule must behave identically for every stored row.
  const contentType = canonicalizeContentType(file.type)
  const isImage = isAcceptedImageContentType(contentType)

  if (imagesOnly && !isImage) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Unsupported image type`,
    })
  }

  if (!imagesOnly && file.size === 0) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `File is empty`,
    })
  }

  const maxBytes = imagesOnly
    ? maxImageUploadBytes
    : getMaxUploadBytesForContentType(contentType)

  if (file.size > maxBytes) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: isImage
        ? `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller`
        : `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller`,
    })
  }

  await assertWithinStorageLimit(issueContext.teamId, file.size)

  // Browser file names arrive verbatim — strip control chars and clamp so the
  // stored display name is always header- and column-safe.
  const filename = sanitizeUploadFilename(
    file.name,
    imagesOnly ? `image` : `file`
  )
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
  // Non-image uploads are never probed — width/height stay null.
  const dimensions = isImage ? getImageDimensions(body) : null

  await uploadObject({
    body,
    contentLength: file.size,
    contentType,
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
    contentType,
    sizeBytes: file.size,
    width: dimensions?.width ?? null,
    height: dimensions?.height ?? null,
  })
}
