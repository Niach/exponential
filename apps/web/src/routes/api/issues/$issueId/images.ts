import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleIssueAttachmentUpload } from "@/lib/storage/issue-attachment-upload"

// Legacy image-only upload route. Its external contract (status codes, error
// strings, response JSON) is BYTE-FROZEN — old native builds still post here.
// New clients use POST /api/issues/$issueId/files.
export const Route = createFileRoute(`/api/issues/$issueId/images`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleIssueAttachmentUpload(context, {
            imagesOnly: true,
          })
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
