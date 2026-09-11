import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import {
  buildAttachmentPosterStorageKey,
  buildAttachmentPosterUrl,
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  canonicalizeContentType,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  isInlineMediaContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
  maxPosterUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import {
  getVideoMetadata,
  isProbeableVideoContentType,
  type VideoMetadata,
} from "@/lib/storage/video-metadata"
import { uploadObject, deleteObject } from "@/lib/storage"
import { assertTeamMember, getIssueTeamContext } from "@/lib/team-membership"
import { assertWithinStorageLimit } from "@/lib/billing"

export interface IssueAttachmentUploadContext {
  params: { issueId: string }
  request: Request
}

// EXP-824: bounds for the OPTIONAL client-probed media metadata parts
// (`width`, `height`, `durationMs`). The normalising client (Mediabunny /
// AVFoundation / Media3) knows these for containers the server does not parse
// (webm); they are attacker-controlled form fields headed for int4 columns,
// so they get the same plausibility bound as the header probes.
const MAX_CLIENT_DIMENSION = 65535
const MAX_CLIENT_DURATION_MS = 48 * 60 * 60 * 1000

function readBoundedInt(formData: FormData, name: string, max: number) {
  const raw = formData.get(name)
  if (typeof raw !== `string` || !/^\d{1,12}$/.test(raw.trim())) return null
  const value = Number.parseInt(raw.trim(), 10)
  if (!Number.isFinite(value) || value < 1 || value > max) return null
  return value
}

/**
 * Resolves a video/audio upload's metadata: the server-side header probe wins
 * for MP4/MOV; the client-supplied fields fill in for anything it can't parse
 * (webm) or when the probe fails. Never blocks the upload.
 */
function resolveMediaMetadata(
  contentType: string,
  body: Uint8Array,
  formData: FormData
): VideoMetadata {
  const probed = isProbeableVideoContentType(contentType)
    ? getVideoMetadata(body)
    : null
  const clientWidth = readBoundedInt(formData, `width`, MAX_CLIENT_DIMENSION)
  const clientHeight = readBoundedInt(formData, `height`, MAX_CLIENT_DIMENSION)
  const clientDuration = readBoundedInt(
    formData,
    `durationMs`,
    MAX_CLIENT_DURATION_MS
  )
  const hasProbedSize = probed?.width != null && probed.height != null
  const hasClientSize = clientWidth !== null && clientHeight !== null
  return {
    width: hasProbedSize ? probed!.width : hasClientSize ? clientWidth : null,
    height: hasProbedSize ? probed!.height : hasClientSize ? clientHeight : null,
    durationMs: probed?.durationMs ?? clientDuration,
    videoCodec: probed?.videoCodec ?? null,
    audioCodec: probed?.audioCodec ?? null,
  }
}

/**
 * Shared body of the `/files` issue upload route (EXP-297): any content type,
 * 10 MB for the inline image types and 50 MB for everything else. EXP-824:
 * a `video/*` or `audio/*` part is probed for width/height/duration and may
 * carry an optional `poster` part (an accepted image type, ≤2 MB) that is
 * stored beside the bytes and served as `?poster=1`.
 */
export async function handleIssueAttachmentUpload({
  params,
  request,
}: IssueAttachmentUploadContext) {
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
    // branch. Name the failure instead of a bare "Missing file".
    const mangled = [...formData.keys()].some((key) => key.startsWith(`file;`))
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: mangled
        ? `Unsupported multipart encoding (unquoted disposition). Update the app.`
        : `Missing file`,
    })
  }

  // Canonicalized before any classification: picker-/provider-supplied types
  // are not guaranteed lowercase or parameter-free, and the exact-match
  // inline-image rule must behave identically for every stored row.
  const contentType = canonicalizeContentType(file.type)
  const isImage = isAcceptedImageContentType(contentType)
  const isMedia = isInlineMediaContentType(contentType)

  if (file.size === 0) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `File is empty`,
    })
  }

  const maxBytes = getMaxUploadBytesForContentType(contentType)

  if (file.size > maxBytes) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: isImage
        ? `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller`
        : `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller`,
    })
  }

  // The poster part only means anything on a media upload; anywhere else it
  // is ignored rather than rejected (an older client never sends one).
  const posterPart = isMedia ? formData.get(`poster`) : null
  const poster =
    posterPart instanceof File &&
    posterPart.size > 0 &&
    isAcceptedImageContentType(canonicalizeContentType(posterPart.type))
      ? posterPart
      : null

  if (poster && poster.size > maxPosterUploadBytes) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Poster frames must be ${maxPosterUploadBytes / (1024 * 1024)} MB or smaller`,
    })
  }

  await assertWithinStorageLimit(
    issueContext.teamId,
    file.size + (poster?.size ?? 0)
  )

  // Browser file names arrive verbatim — strip control chars and clamp so the
  // stored display name is always header- and column-safe.
  const filename = sanitizeUploadFilename(file.name, `file`)
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
  // Media (EXP-824): MP4/MOV headers are probed the same way; client-supplied
  // fields fill in for containers the server does not parse.
  const media = isMedia ? resolveMediaMetadata(contentType, body, formData) : null
  const dimensions = isImage ? getImageDimensions(body) : media
  const posterStorageKey = poster
    ? buildAttachmentPosterStorageKey(storageKey)
    : null

  await uploadObject({
    body,
    contentLength: file.size,
    contentType,
    key: storageKey,
  })

  const rollbackObjects = async () => {
    for (const key of [storageKey, posterStorageKey]) {
      if (!key) continue
      try {
        await deleteObject(key)
      } catch (deleteError) {
        console.error(
          `Failed to rollback uploaded attachment object`,
          deleteError
        )
      }
    }
  }

  if (poster && posterStorageKey) {
    try {
      await uploadObject({
        body: new Uint8Array(await poster.arrayBuffer()),
        contentLength: poster.size,
        contentType: canonicalizeContentType(poster.type),
        key: posterStorageKey,
      })
    } catch (error) {
      await rollbackObjects()
      throw error
    }
  }

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
      durationMs: media?.durationMs ?? null,
      posterStorageKey,
      posterSizeBytes: poster && posterStorageKey ? poster.size : null,
    })
  } catch (error) {
    await rollbackObjects()
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
    durationMs: media?.durationMs ?? null,
    posterUrl: posterStorageKey ? buildAttachmentPosterUrl(attachmentId) : null,
    // Sample-entry fourccs so the client can show the "may not play
    // everywhere" hint for anything outside H.264 (`avc1`/`avc3`) + AAC.
    videoCodec: media?.videoCodec ?? null,
    audioCodec: media?.audioCodec ?? null,
  })
}
