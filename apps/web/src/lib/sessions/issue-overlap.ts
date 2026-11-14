import { and, asc, eq, inArray, ne } from "drizzle-orm"
import { overlapWarning } from "@exp/eagent/overlap"
import { db } from "@/db/connection"
import { issues, sessionIssues, sessions } from "@/db/schema"
import { sessionVisibleTo } from "@/lib/sessions/access"

// #NEW-18 (packages/eagent/fixtures/overlap.json): a send naming issues
// another live session already works on, or that are done, says so in its
// result's `warning`. Read AFTER the send committed and never throws: the
// warning never blocks a start.

const DONE = new Set([`done`, `cancelled`, `duplicate`])

export async function issueOverlapWarning(args: {
  userId: string
  sessionId: string
  issueIds: readonly string[] | undefined
}): Promise<string | null> {
  const ids = [...new Set(args.issueIds ?? [])]
  if (ids.length === 0) return null
  try {
    const [landed] = await db
      .select({ rootId: sessions.rootId })
      .from(sessions)
      .where(eq(sessions.id, args.sessionId))
      .limit(1)
    if (!landed) return null
    const [issueRows, holderRows] = await Promise.all([
      db
        .select({ id: issues.id, identifier: issues.identifier, status: issues.status })
        .from(issues)
        .where(inArray(issues.id, ids)),
      db
        .select({
          issueId: sessionIssues.issueId,
          sessionId: sessions.id,
          shared: sessions.shared,
          userId: sessions.userId,
        })
        .from(sessionIssues)
        .innerJoin(sessions, eq(sessions.id, sessionIssues.sessionId))
        .where(
          and(
            inArray(sessionIssues.issueId, ids),
            ne(sessions.status, `ended`),
            ne(sessions.rootId, landed.rootId)
          )
        )
        .orderBy(asc(sessions.createdAt), asc(sessions.id)),
    ])
    const byId = new Map(issueRows.map((row) => [row.id, row]))
    return overlapWarning(
      ids.flatMap((id) => {
        const issue = byId.get(id)
        if (!issue) return []
        return [
          {
            identifier: issue.identifier,
            done: DONE.has(issue.status),
            holders: holderRows
              .filter((row) => row.issueId === id)
              .map((row) => ({ sessionId: row.sessionId, visible: sessionVisibleTo(row, args.userId) })),
          },
        ]
      })
    )
  } catch {
    return null
  }
}
