import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import { prepareElectricUrl, proxyElectricRequest } from "@/lib/electric-proxy"
import { getUserProjectIds, buildWhereClause } from "@/lib/workspace-membership"

export const Route = createFileRoute(`/api/shapes/issues`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const session = await auth.api.getSession({
          headers: request.headers,
        })
        if (!session) {
          return new Response(`Unauthorized`, { status: 401 })
        }

        const projectIds = await getUserProjectIds(session.user.id)
        const originUrl = prepareElectricUrl(request.url)
        originUrl.searchParams.set(`table`, `issues`)
        originUrl.searchParams.set(
          `where`,
          buildWhereClause(`project_id`, projectIds)
        )

        return proxyElectricRequest(originUrl)
      },
    },
  },
})
