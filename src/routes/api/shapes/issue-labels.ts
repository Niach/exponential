import { createFileRoute } from "@tanstack/react-router"
import { getUserLabelIds, buildWhereClause } from "@/lib/workspace-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

export const Route = createFileRoute(`/api/shapes/issue-labels`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `issue_labels`,
        getWhere: async (userId) => {
          const labelIds = await getUserLabelIds(userId)
          return buildWhereClause(`label_id`, labelIds)
        },
      }),
    },
  },
})
