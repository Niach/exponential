import { and, asc, inArray, isNull, or, sql } from "drizzle-orm"
import { sessionTitles, type TitleSession } from "@exp/domain-contract/session-title"
import { db } from "@/db/connection"
import { issues, messages, sessionIssues } from "@/db/schema"

// EXP-1341: the server's ONE display-title derivation = the contract's
// `sessionTitle` (fixtures/session-title.json) over the same rows the
// clients sync: the trees' issue links (a child = its ASSIGNED ones) and each
// session's input messages. Batched: one read per table for any set.

type Reader = Pick<typeof db, `select`>

export async function deriveSessionTitles(
  rows: readonly TitleSession[],
  database: Reader = db
): Promise<Map<string, string>> {
  if (rows.length === 0) return new Map()
  const rootIds = [...new Set(rows.map((row) => row.rootId))]
  const ids = rows.map((row) => row.id)
  const needsLinks = rows.some((row) => !row.name?.trim() && !row.title?.trim())
  const [links, inputs] = needsLinks
    ? await Promise.all([
        database
          .select({
            sessionId: sessionIssues.sessionId,
            issueId: sessionIssues.issueId,
            sortOrder: sessionIssues.sortOrder,
            assignedSessionId: sessionIssues.assignedSessionId,
            id: issues.id,
            identifier: issues.identifier,
            title: issues.title,
          })
          .from(sessionIssues)
          .innerJoin(issues, sql`${issues.id} = ${sessionIssues.issueId}`)
          .where(inArray(sessionIssues.sessionId, rootIds)),
        database
          .select({
            id: messages.id,
            sessionId: messages.sessionId,
            authorKind: messages.authorKind,
            authorSessionId: messages.authorSessionId,
            body: messages.body,
            createdAt: messages.createdAt,
          })
          .from(messages)
          .where(
            and(
              inArray(messages.sessionId, ids),
              isNull(messages.card),
              or(
                sql`${messages.authorKind} = 'user'`,
                sql`${messages.authorSessionId} IS NOT NULL AND ${messages.authorSessionId} <> ${messages.sessionId}`
              )
            )
          )
          .orderBy(asc(messages.createdAt)),
      ])
    : [[], []]
  return sessionTitles(
    rows,
    links,
    links,
    inputs.map((m) => ({ ...m, card: null, createdAt: m.createdAt.toISOString() }))
  )
}

export async function deriveSessionTitle(row: TitleSession, database: Reader = db): Promise<string> {
  return (await deriveSessionTitles([row], database)).get(row.id)!
}
