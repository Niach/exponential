import { createFileRoute } from "@tanstack/react-router"
import { APP_SHARE_COPY } from "@exp/domain-contract/app-share"
import { errorToResponse } from "@/lib/http-errors"
import { guestPage, handleShareSubmit } from "@/lib/apps/share-links"

// VAPP-241 (eApps P1, lib/apps/share-links.ts): a public link's guest API, no
// session: GET = what the page renders, POST = one submission. Every link that
// is not live answers the same 404, like an unknown token.
export const Route = createFileRoute(`/api/share/$token`)({
  server: {
    handlers: {
      GET: async ({ params }) => {
        try {
          const page = await guestPage(params.token)
          if (!page) return Response.json({ error: APP_SHARE_COPY.gone }, { status: 404, headers: { "cache-control": `no-store` } })
          return Response.json(page, { headers: { "cache-control": `no-store` } })
        } catch (error) {
          return errorToResponse(error)
        }
      },
      POST: async ({ request, params }) => {
        try {
          return await handleShareSubmit(request, params.token)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
