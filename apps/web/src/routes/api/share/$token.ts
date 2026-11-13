import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleSharePage, handleShareSubmit } from "@/lib/apps/share-links"

// VAPP-241 (eApps P1, lib/apps/share-links.ts): a public link's guest API, no
// session: GET = what the page renders, POST = one submission. Every link that
// is not live answers the same 404, like an unknown token.
export const Route = createFileRoute(`/api/share/$token`)({
  server: {
    handlers: {
      GET: async ({ request, params }) => {
        try {
          return await handleSharePage(request, params.token)
        } catch (error) {
          return errorToResponse(error, request)
        }
      },
      POST: async ({ request, params }) => {
        try {
          return await handleShareSubmit(request, params.token)
        } catch (error) {
          return errorToResponse(error, request)
        }
      },
    },
  },
})
