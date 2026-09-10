import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleTeamSessionAttachmentUpload } from "@/lib/storage/session-attachment-upload"

// EXP-825: an image attached to a START before its session exists — the
// Agent page composer uploads here, embeds the id in the start's `prompt`,
// and the device binds the row to the session it creates. Images only, team
// members only. One multipart part named "file", same response contract as
// the session and issue `/files` routes.
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
