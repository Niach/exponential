import { createFileRoute } from "@tanstack/react-router"
import {
  buildSessionVisibleWhere,
  getUserTeamIds,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// NEW-3: the chat primitive's rows — in step 1 every conversation is a
// session's own (kind `session`), so a row is visible exactly when its
// session is: the mirrored `shared` / `owner_id` replay the session rule
// (`buildSessionVisibleWhere`). Step 2 (NEW-8) adds dm/channel rows whose
// readers are their `conversation_members` (server-only until then).
export const CONVERSATION_COLUMNS = [
  `id`,
  `team_id`,
  `kind`,
  `session_id`,
  `title`,
  `created_at`,
  `updated_at`,
]

export const Route = createFileRoute(`/api/shapes/conversations`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `conversations`,
        columns: CONVERSATION_COLUMNS,
        getWhere: async (userId) => {
          const teamIds = userId ? await getUserTeamIds(userId) : []
          return buildSessionVisibleWhere(teamIds, userId, `owner_id`)
        },
      }),
    },
  },
})
