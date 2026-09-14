import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleDraftAttachmentUpload } from "@/lib/storage/draft-attachment-upload"

// EXP-878: eager upload into an issue DRAFT, before the issue exists. Same
// multipart contract, caps and response shape as the issue `/files` route;
// owner-only (a draft is private to the person composing it). The created
// issue adopts the rows via `issues.create({ draftId })`.
export const Route = createFileRoute(`/api/issue-drafts/$draftId/files`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleDraftAttachmentUpload(context)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
