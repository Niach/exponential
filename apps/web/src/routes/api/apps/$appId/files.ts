import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleAppFileUpload } from "@/lib/apps/app-files"

// VAPP-221 (eApps R7): a file for an app's records (lib/apps/app-files.ts).
export const Route = createFileRoute(`/api/apps/$appId/files`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleAppFileUpload(context)
        } catch (error) {
          return errorToResponse(error, context.request)
        }
      },
    },
  },
})
