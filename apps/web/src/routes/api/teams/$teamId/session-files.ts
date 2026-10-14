import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleTeamSessionAttachmentUpload } from "@/lib/storage/session-attachment-upload"

// EXP-825/1291: an image or file attached to a START before its session
// exists — the composer uploads here and `sessions.send({attachmentIds})`
// claims the row onto the start's first message. Any type (images 10 MB,
// files 50 MB), team members only. One multipart part named "file", same
// response contract as the session and issue `/files` routes.
export const Route = createFileRoute(`/api/teams/$teamId/session-files`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleTeamSessionAttachmentUpload(context)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
