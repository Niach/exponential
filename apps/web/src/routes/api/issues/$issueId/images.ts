import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleIssueAttachmentUpload } from "@/lib/storage/issue-attachment-upload"

// Legacy image-only upload route. Its external contract (status codes, error
// strings, response JSON) is BYTE-FROZEN — shipped native builds up to the
// EXP-613 migration (iOS <= 0.14.14, Android <= 0.14.17, desktop <= 0.14.22)
// still post inline images here; every current client uses
// POST /api/issues/$issueId/files. Removable once those floors pass
// (EXP-613/EXP-589).
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
