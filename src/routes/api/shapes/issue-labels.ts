import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import { prepareElectricUrl, proxyElectricRequest } from "@/lib/electric-proxy"
import { getUserLabelIds, buildWhereClause } from "@/lib/workspace-membership"

export const Route = createFileRoute(`/api/shapes/issue-labels`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const session = await auth.api.getSession({
          headers: request.headers,
        })
        if (!session) {
          return new Response(`Unauthorized`, { status: 401 })
        }

        const labelIds = await getUserLabelIds(session.user.id)
        const originUrl = prepareElectricUrl(request.url)
        originUrl.searchParams.set(`table`, `issue_labels`)
        originUrl.searchParams.set(
          `where`,
          buildWhereClause(`label_id`, labelIds)
        )

        return proxyElectricRequest(originUrl)
      },
    },
  },
})
