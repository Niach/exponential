import { createFileRoute } from "@tanstack/react-router"
import {
  andClauses,
  buildWhereClause,
  getUserBoardIds,
  getUserTeamIds,
  orClauses,
} from "@/lib/team-membership"
import { createShapeRouteHandler } from "@/lib/shape-route"

// Live "coding now" rows. MEMBER-ONLY: anonymous callers get NOTHING (the
// anonymous branch's empty id list yields the impossible-match sentinel —
// zero rows, no 401).
//
// Members sync sessions in their teams that are either batch-scoped
// (board_id NULL — a batch run spans boards and carries no board
// identity) or belong to a non-trashed board. Issue-scoped sessions of a
// trashed board therefore hide for the 48h trash window along with the
// board itself. team_id is trigger-denormalized, as is board_id
// except on batch rows where it is deliberately null.
export const Route = createFileRoute(`/api/shapes/coding-sessions`)({
  server: {
    handlers: {
      GET: createShapeRouteHandler({
        table: `coding_sessions`,
        getWhere: async (userId) => {
          // Anonymous clause stays byte-identical via this early return.
          if (!userId) return buildWhereClause(`team_id`, [])
          const teamIds = await getUserTeamIds(userId)
          const boardIds = await getUserBoardIds(userId)
          return andClauses(
            buildWhereClause(`team_id`, teamIds),
            orClauses(
              `"board_id" IS NULL`,
              buildWhereClause(`board_id`, boardIds)
            )
          )
        },
      }),
    },
  },
})
