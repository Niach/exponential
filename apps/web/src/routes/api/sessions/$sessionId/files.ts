import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleSessionAttachmentUpload } from "@/lib/storage/session-attachment-upload"

// EXP-702/1291: a file for a message INTO a session (any type, images
// 10 MB, files 50 MB): an owner-less team upload `sessions.send
// ({attachmentIds})` claims onto the message. Whoever may post in the
// session. One multipart part named "file", same response contract as the
// issue `/files` route.
export const Route = createFileRoute(`/api/sessions/$sessionId/files`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleSessionAttachmentUpload(context)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
