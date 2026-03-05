import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import { prepareElectricUrl, proxyElectricRequest } from "@/lib/electric-proxy"
import {
  getUserIdsInWorkspaces,
  buildWhereClause,
} from "@/lib/workspace-membership"

export const Route = createFileRoute(`/api/shapes/users`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const session = await auth.api.getSession({
          headers: request.headers,
        })
        if (!session) {
          return new Response(`Unauthorized`, { status: 401 })
        }

        const userIds = await getUserIdsInWorkspaces(session.user.id)
        const originUrl = prepareElectricUrl(request.url)
        originUrl.searchParams.set(`table`, `users`)
        originUrl.searchParams.set(`where`, buildWhereClause(`id`, userIds))

        return proxyElectricRequest(originUrl)
      },
    },
  },
})
