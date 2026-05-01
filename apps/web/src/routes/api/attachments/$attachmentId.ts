import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
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
  const session = await auth.api.getSession({
    headers: request.headers,
  })

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
