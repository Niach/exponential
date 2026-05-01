import { TRPCError } from "@trpc/server"
import { createFileRoute } from "@tanstack/react-router"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { auth } from "@/lib/auth"
import { errorToResponse } from "@/lib/http-errors"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  isAcceptedImageContentType,
  maxImageUploadBytes,
} from "@/lib/issue-attachments"
import { uploadObject, deleteObject } from "@/lib/storage"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"

async function uploadIssueImage({
  params,
  request,
}: {
  params: { issueId: string }
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

  const issueContext = await getIssueWorkspaceContext(params.issueId)
  await assertWorkspaceMember(session.user.id, issueContext.workspaceId)

  const formData = await request.formData()
  const file = formData.get(`file`)

  if (!(file instanceof File)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Missing image file`,
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

  const attachmentId = crypto.randomUUID()
  const storageKey = buildAttachmentStorageKey(
    params.issueId,
    attachmentId,
    file.name
  )
  const url = buildAttachmentUrl(attachmentId)
  const body = new Uint8Array(await file.arrayBuffer())

  await uploadObject({
    body,
    contentLength: file.size,
    contentType: file.type,
    key: storageKey,
  })

  try {
    await db.insert(attachments).values({
      id: attachmentId,
      issueId: params.issueId,
      uploaderId: session.user.id,
      filename: file.name,
      contentType: file.type,
      sizeBytes: file.size,
      storageKey,
      url,
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
    filename: file.name,
    contentType: file.type,
    sizeBytes: file.size,
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
