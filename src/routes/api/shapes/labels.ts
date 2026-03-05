import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import { prepareElectricUrl, proxyElectricRequest } from "@/lib/electric-proxy"
import {
  getUserWorkspaceIds,
  buildWhereClause,
} from "@/lib/workspace-membership"

export const Route = createFileRoute(`/api/shapes/labels`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const session = await auth.api.getSession({
          headers: request.headers,
        })
        if (!session) {
          return new Response(`Unauthorized`, { status: 401 })
        }

        const workspaceIds = await getUserWorkspaceIds(session.user.id)
        const originUrl = prepareElectricUrl(request.url)
        originUrl.searchParams.set(`table`, `labels`)
        originUrl.searchParams.set(
          `where`,
          buildWhereClause(`workspace_id`, workspaceIds)
        )

        return proxyElectricRequest(originUrl)
      },
    },
  },
})
