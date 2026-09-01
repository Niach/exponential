import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { errorToResponse } from "@/lib/http-errors"
import { getObject, toResponseBody } from "@/lib/storage"
import {
  buildContentDispositionHeader,
  isInlineSafeContentType,
} from "@/lib/storage/issue-attachments"
import { verifyAttachmentToken } from "@/lib/storage/attachment-token"
import {
  assertTeamMember,
  getAttachmentTeamContext,
  getSessionAttachmentTeamContext,
} from "@/lib/team-membership"

const singleRangePattern = /^bytes=(\d*)-(\d*)$/

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

interface ParsedRange {
  header: string
  satisfiable: boolean
}

/**
 * Parses a single byte-range request. Returns null for anything we don't
 * support (multi-range, other units, garbage) — RFC 9110 says an unparsable
 * Range must be IGNORED, so those fall back to a normal 200 full-body read.
 * A syntactically valid but out-of-bounds range is reported as unsatisfiable
 * so the caller can answer 416.
 */
function parseRangeHeader(
  rangeHeader: string | null,
  sizeBytes: number
): ParsedRange | null {
  if (!rangeHeader) return null

  const match = singleRangePattern.exec(rangeHeader.trim())
  if (!match) return null

  const rawStart = match[1] ?? ``
  const rawEnd = match[2] ?? ``
  if (!rawStart && !rawEnd) return null

  if (!rawStart) {
    // Suffix range: `bytes=-N` (last N bytes).
    const suffixLength = Number.parseInt(rawEnd, 10)
    if (!Number.isFinite(suffixLength)) return null
    return { header: rangeHeader, satisfiable: suffixLength > 0 }
  }

  const start = Number.parseInt(rawStart, 10)
  if (!Number.isFinite(start)) return null

  if (rawEnd) {
    const end = Number.parseInt(rawEnd, 10)
    if (!Number.isFinite(end) || end < start) {
      return { header: rangeHeader, satisfiable: false }
    }
  }

  // sizeBytes is 0 for legacy rows that never recorded a size — skip the
  // bounds check there and let S3 be the authority.
  const satisfiable = sizeBytes <= 0 || start < sizeBytes
  return { header: rangeHeader, satisfiable }
}

async function getAttachment({
  params,
  request,
}: {
  params: { attachmentId: string }
  request: Request
}) {
  // Attachment URLs appear in issue/comment markdown that MCP clients read,
  // so this route must accept every credential the MCP endpoint accepts other
  // than OAuth2 access tokens (deliberately confined to /api/mcp): expu_ api
  // keys and session cookie/bearer — resolveSession is the shared chokepoint
  // and also downgrades auth-plugin throws to null instead of leaking them as
  // 500s.
  const session = await resolveSession(request)

  // EXP-704: a signed download token is an alternative credential, scoped to
  // exactly this attachment. MCP attachments_get mints it AFTER running its
  // OAuth board-grant confinement plus the membership check, so a valid,
  // unexpired token bound to this path id is a complete authorization
  // decision — no session and no re-run of assertTeamMember (the ~10 min TTL
  // bounds a member losing access mid-window). A token for a DIFFERENT id
  // grants nothing here.
  const tokenParam = new URL(request.url).searchParams.get(`token`)
  const tokenAuthorized = Boolean(
    tokenParam &&
      verifyAttachmentToken(tokenParam)?.attachmentId === params.attachmentId
  )

  // Member-only: attachment bytes are never anonymously readable. The 401
  // comes BEFORE any DB lookup (REV-49) so anonymous probes get a uniform
  // answer whether or not the id exists — no existence oracle, and matches
  // the historic no-session behavior (an invalid/expired token is just
  // anonymous).
  if (!session?.user && !tokenAuthorized) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: `Unauthorized`,
    })
  }

  // The id column is uuid — an unvalidated path segment would make Postgres
  // throw 22P02 (not a TRPCError), turning every crawler hit on a garbage
  // attachment URL into a logged 500 instead of this 404.
  if (!UUID_RE.test(params.attachmentId)) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Attachment not found`,
    })
  }

  // Issue attachments first; steer images (EXP-702) live in the server-only
  // session_attachments table but are served from the
  // SAME url shape — the EXP-511 embed `![image](/api/attachments/{id})` is
  // load-bearing across every host and viewer, so the fallback happens here
  // rather than in a second route.
  const attachment = await getAttachmentTeamContext(params.attachmentId).catch(
    (error: unknown) => {
      if (error instanceof TRPCError && error.code === `NOT_FOUND`) {
        return getSessionAttachmentTeamContext(params.attachmentId)
      }
      throw error
    }
  )
  if (!tokenAuthorized) {
    await assertTeamMember(session!.user.id, attachment.teamId)
  }

  // An empty stored type must never be sniffed by the browser.
  const contentType = attachment.contentType || `application/octet-stream`
  const forceDownload =
    new URL(request.url).searchParams.get(`download`) === `1`
  // Only a narrow allowlist may render in a browsing context; everything else
  // (text/html, image/svg+xml, unknown types) downloads instead (EXP-297).
  const disposition =
    !forceDownload && isInlineSafeContentType(contentType)
      ? `inline`
      : `attachment`

  const baseHeaders = {
    "Accept-Ranges": `bytes`,
    "Cache-Control": `private, max-age=3600`,
    // RFC 6266/5987-encoded: stored filenames may contain non-Latin-1 (or,
    // pre-sanitization, control) characters that would make the Headers
    // constructor throw and 500 every read of the attachment.
    "Content-Disposition": buildContentDispositionHeader(
      disposition,
      attachment.filename
    ),
    "Content-Type": contentType,
    "X-Content-Type-Options": `nosniff`,
  }

  const range = parseRangeHeader(
    request.headers.get(`range`),
    attachment.sizeBytes
  )

  if (range && !range.satisfiable) {
    return new Response(null, {
      status: 416,
      headers: new Headers({
        ...baseHeaders,
        "Content-Range": `bytes */${attachment.sizeBytes}`,
      }),
    })
  }

  const object = await getObject(
    attachment.storageKey,
    range ? { range: range.header } : undefined
  )

  if (!object) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Attachment not found`,
    })
  }

  const body = await toResponseBody(object.Body)

  if (!body) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Attachment not found`,
    })
  }

  const headers = new Headers(baseHeaders)

  // A partial response is only claimed when S3 actually honored the range.
  const isPartial = Boolean(range && object.ContentRange)

  if (isPartial) {
    headers.set(`Content-Range`, object.ContentRange!)

    if (typeof object.ContentLength === `number`) {
      headers.set(`Content-Length`, object.ContentLength.toString())
    }

    return new Response(body, { status: 206, headers })
  }

  if (attachment.sizeBytes > 0) {
    headers.set(`Content-Length`, attachment.sizeBytes.toString())
  }

  return new Response(body, {
    headers,
  })
}

export const Route = createFileRoute(`/api/attachments/$attachmentId`)({
  server: {
    handlers: {
      GET: async (context) => {
        try {
          return await getAttachment(context)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
