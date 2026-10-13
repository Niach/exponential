import { and, eq, inArray, isNull } from "drizzle-orm"
import { boards, issues, pullRequests, repositories } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import type { ActionInputLookups } from "@/lib/action-inputs"
import type { Db } from "@/lib/sessions/tx"

// EXP-1291: the DB half of action-input resolution — every pick is scoped
// to the session's team (`sessions.send` validates, the brief re-resolves
// the displays at launch).
export function actionInputLookups(db: Db): ActionInputLookups {
  return {
    repo: async (id, teamId) => {
      const [row] = await db
        .select({ fullName: repositories.fullName })
        .from(repositories)
        .where(
          and(
            eq(repositories.id, id),
            eq(repositories.teamId, teamId),
            isNull(repositories.archivedAt)
          )
        )
        .limit(1)
      return row ?? null
    },
    board: async (id, teamId) => {
      const [row] = await db
        .select({ name: boards.name })
        .from(boards)
        .where(and(eq(boards.id, id), eq(boards.teamId, teamId), boardVisible()))
        .limit(1)
      return row ?? null
    },
    pr: async (id, teamId) => {
      const [row] = await db
        .select({ number: pullRequests.number, title: pullRequests.title })
        .from(pullRequests)
        .where(
          and(
            eq(pullRequests.id, id),
            eq(pullRequests.teamId, teamId),
            inArray(pullRequests.state, [`open`, `draft`])
          )
        )
        .limit(1)
      return row ?? null
    },
    issue: async (id, teamId) => {
      const [row] = await db
        .select({ identifier: issues.identifier, title: issues.title })
        .from(issues)
        .innerJoin(boards, eq(boards.id, issues.boardId))
        .where(and(eq(issues.id, id), eq(boards.teamId, teamId), boardVisible()))
        .limit(1)
      return row ?? null
    },
  }
}
