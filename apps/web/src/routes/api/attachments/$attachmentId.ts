import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { errorToResponse } from "@/lib/http-errors"
import { getObject, toResponseBody } from "@/lib/storage"
import { buildContentDispositionHeader } from "@/lib/storage/issue-attachments"
import {
  assertWorkspaceMember,
  getAttachmentWorkspaceContext,
} from "@/lib/workspace-membership"

async function getAttachment({
  params,
  request,
}: {
  params: { attachmentId: string }
  request: Request
}) {
  // Attachment URLs appear in issue/comment markdown that MCP clients read, so
  // this route must accept every credential the MCP endpoint accepts (OAuth2
  // access tokens, expu_ api keys, session cookie/bearer) — resolveSession is
  // the shared chokepoint and also downgrades auth-plugin throws to null
  // instead of leaking them as 500s.
  const session = await resolveSession(request)

  const attachment = await getAttachmentWorkspaceContext(params.attachmentId)

  if (session?.user) {
    await assertWorkspaceMember(session.user.id, attachment.workspaceId)
  } else {
    // Anonymous byte reads are allowed ONLY for public feedback boards
    // (inline images in issue descriptions must load for logged-out
    // visitors); comment attachments additionally require the board to show
    // comments publicly. Same predicate as the attachments shape's anonymous
    // where clause. 401 (not 404) to match the historic no-session behavior
    // and avoid an existence oracle.
    const publiclyReadable =
      attachment.projectType === `feedback` &&
      attachment.projectArchivedAt === null &&
      (attachment.commentId === null || attachment.projectPublicShowComments)
    if (!publiclyReadable) {
      throw new TRPCError({
        code: `UNAUTHORIZED`,
        message: `Unauthorized`,
      })
    }
  }

  const object = await getObject(attachment.storageKey)

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

  const headers = new Headers({
    "Cache-Control": `private, max-age=3600`,
    // RFC 6266/5987-encoded: stored filenames may contain non-Latin-1 (or,
    // pre-sanitization, control) characters that would make the Headers
    // constructor throw and 500 every read of the attachment.
    "Content-Disposition": buildContentDispositionHeader(
      `inline`,
      attachment.filename
    ),
    "Content-Type": attachment.contentType,
  })

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
