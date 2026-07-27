import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleIssueAttachmentUpload } from "@/lib/storage/issue-attachment-upload"

// EXP-297: any-content-type issue attachment upload (10 MB for inline images,
// 50 MB for everything else). Same multipart contract as /images — one part
// named "file" — and the same response shape.
export const Route = createFileRoute(`/api/issues/$issueId/files`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleIssueAttachmentUpload(context, {
            imagesOnly: false,
          })
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
