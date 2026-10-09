import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleSessionAttachmentUpload } from "@/lib/storage/session-attachment-upload"

// EXP-702: steer attachment upload — every steered image or file (wave D:
// any type, images 10 MB, files 50 MB), issue runs included, so steering
// attachments stay out of the issue's Files section. Session owner only. One
// multipart part named "file", same response contract as the issue `/files`
// route.
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
