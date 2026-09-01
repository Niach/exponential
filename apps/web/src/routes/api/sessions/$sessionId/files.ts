import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleSessionAttachmentUpload } from "@/lib/storage/session-attachment-upload"

// EXP-702: steer-image upload for coding sessions without an issue (chat,
// action and batch runs). Images only, session owner only. One multipart
// part named "file", same response contract as the issue `/files` route.
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
