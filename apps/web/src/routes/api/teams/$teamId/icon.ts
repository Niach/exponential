import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleTeamIconUpload } from "@/lib/storage/team-icon-upload"

// #EXP-1645 (fixtures/team-image.json): the team image. Team owners only; one
// multipart part named "file"; answers {iconUrl}.
export const Route = createFileRoute(`/api/teams/$teamId/icon`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleTeamIconUpload(context)
        } catch (error) {
          return errorToResponse(error, context.request)
        }
      },
    },
  },
})
