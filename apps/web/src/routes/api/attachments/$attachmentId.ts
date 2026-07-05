import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { errorToResponse } from "@/lib/http-errors"
import { getObject, toResponseBody } from "@/lib/storage"
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

  if (!session?.user) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: `Unauthorized`,
    })
  }

  const attachment = await getAttachmentWorkspaceContext(params.attachmentId)
  await assertWorkspaceMember(session.user.id, attachment.workspaceId)

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
    "Content-Disposition": `inline; filename="${attachment.filename.replace(/"/g, `'`)}"`,
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
