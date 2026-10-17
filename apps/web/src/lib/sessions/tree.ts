import { and, asc, desc, eq, inArray, isNull, ne, or } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  issues,
  messages,
  pullRequests,
  sessionIssues,
  sessions,
  type Session,
} from "@/db/schema"
import { getUserTeamIds } from "@/lib/auth/membership"
import { loadSessionForUser } from "@/lib/sessions/access"
import { deriveSessionTitles } from "@/lib/sessions/title"
import type { SessionKind } from "@exp/db-schema/domain"

// EXP-1291: the session tree as one read (MCP `sessions_get`) and the thread
// / agent lists (MCP `sessions_list`). Same visibility rule as the shapes;
// the server-only facets (`host_user_id`, `mcp_server_ids`, a dispatcher's
// `work_launch`, an agent's `instructions`) never leave here.

export type PublicSession = Omit<
  Session,
  `hostUserId` | `mcpServerIds` | `workLaunch` | `instructions`
>

export function publicSession(session: Session): PublicSession {
  const {
    hostUserId: _host,
    mcpServerIds: _mcp,
    workLaunch: _work,
    instructions: _instructions,
    ...rest
  } = session
  return rest
}

const DEFAULT_TREE_MESSAGES = 20
const MAX_TREE_MESSAGES = 200

export async function getSessionTree(
  userId: string,
  rootId: string,
  opts: { messages?: number } = {}
) {
  // Any session of the tree resolves to its root.
  const named = await loadSessionForUser(userId, rootId)
  const root =
    named.rootId === named.id ? named : await loadSessionForUser(userId, named.rootId)

  const limit = Math.min(Math.max(opts.messages ?? DEFAULT_TREE_MESSAGES, 0), MAX_TREE_MESSAGES)
  const [issueRows, prRows, childRows, messageRows] = await Promise.all([
    db
      .select({
        issueId: sessionIssues.issueId,
        identifier: issues.identifier,
        title: issues.title,
        status: issues.status,
        source: sessionIssues.source,
        addedBySessionId: sessionIssues.addedBySessionId,
        assignedSessionId: sessionIssues.assignedSessionId,
      })
      .from(sessionIssues)
      .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
      .where(
        and(
          eq(sessionIssues.sessionId, root.id),
          isNull(sessionIssues.boardDeletedAt),
          isNull(sessionIssues.boardArchivedAt)
        )
      )
      .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt)),
    db
      .select()
      .from(pullRequests)
      .where(eq(pullRequests.rootSessionId, root.id))
      .orderBy(asc(pullRequests.createdAt)),
    db
      .select()
      .from(sessions)
      .where(and(eq(sessions.rootId, root.id), ne(sessions.id, root.id)))
      .orderBy(asc(sessions.depth), asc(sessions.createdAt)),
    limit === 0
      ? Promise.resolve([])
      : db
          .select({
            id: messages.id,
            sessionId: messages.sessionId,
            authorKind: messages.authorKind,
            authorId: messages.authorId,
            body: messages.body,
            card: messages.card,
            answeredAt: messages.answeredAt,
            consumedAt: messages.consumedAt,
            createdAt: messages.createdAt,
          })
          .from(messages)
          .where(eq(messages.rootId, root.id))
          .orderBy(desc(messages.createdAt))
          .limit(limit),
  ])

  const titles = await deriveSessionTitles([root, ...childRows])
  return {
    root: { ...publicSession(root), displayTitle: titles.get(root.id)! },
    issues: issueRows,
    pullRequests: prRows,
    guide: root.guide ?? [],
    children: childRows.map((child) => ({
      ...publicSession(child),
      displayTitle: titles.get(child.id)!,
    })),
    // Oldest first, the last `limit` of the whole tree.
    messages: messageRows.reverse(),
  }
}

export interface SessionListRow {
  id: string
  teamId: string
  kind: SessionKind
  title: string
  status: Session[`status`]
  busy: boolean
  needsInput: boolean
  deviceId: string
  userId: string
  shared: boolean
  name: string | null
  actionName: string | null
  createdAt: Date
  updatedAt: Date
}

/** Roots (threads or agents) the caller may see, most recently active
 *  first. `live` = not ended (agents always are). */
export async function listSessions(
  userId: string,
  args: { teamId?: string; kind: SessionKind; live?: boolean; limit: number; offset: number }
): Promise<SessionListRow[]> {
  const memberTeams = await getUserTeamIds(userId)
  const teamIds = args.teamId
    ? memberTeams.filter((id) => id === args.teamId)
    : memberTeams
  if (teamIds.length === 0) return []

  const rows = await db
    .select({
      id: sessions.id,
      teamId: sessions.teamId,
      kind: sessions.kind,
      rootId: sessions.rootId,
      title: sessions.title,
      status: sessions.status,
      busy: sessions.busy,
      needsInput: sessions.needsInput,
      deviceId: sessions.deviceId,
      userId: sessions.userId,
      shared: sessions.shared,
      name: sessions.name,
      actionName: sessions.actionName,
      createdAt: sessions.createdAt,
      updatedAt: sessions.updatedAt,
    })
    .from(sessions)
    .where(
      and(
        inArray(sessions.teamId, teamIds),
        isNull(sessions.parentId),
        eq(sessions.kind, args.kind),
        or(eq(sessions.shared, true), eq(sessions.userId, userId)),
        ...(args.live ? [ne(sessions.status, `ended`)] : [])
      )
    )
    .orderBy(desc(sessions.updatedAt))
    .limit(args.limit)
    .offset(args.offset)
  // EXP-1341: THE title chain (session-title.json), never a SQL copy.
  const titles = await deriveSessionTitles(rows)
  return rows.map(({ rootId: _root, ...row }) => ({ ...row, title: titles.get(row.id)! }))
}
