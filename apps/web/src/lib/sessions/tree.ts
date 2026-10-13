import { and, asc, desc, eq, inArray, isNull, ne, or, sql } from "drizzle-orm"
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
import { deriveSessionTitle } from "@/lib/sessions/brief"
import type { SessionKind } from "@exp/db-schema/domain"

// EXP-1291: the session tree as one read (MCP `sessions_get`) and the thread
// / agent lists (MCP `sessions_list`). Same visibility rule as the shapes;
// the server-only facets (`host_user_id`, `mcp_server_ids`, an agent's
// `instructions`) never leave here.

export type PublicSession = Omit<Session, `hostUserId` | `mcpServerIds` | `instructions`>

export function publicSession(session: Session): PublicSession {
  const { hostUserId: _host, mcpServerIds: _mcp, instructions: _instructions, ...rest } = session
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

  return {
    root: { ...publicSession(root), displayTitle: await deriveSessionTitle(root) },
    issues: issueRows,
    pullRequests: prRows,
    guide: root.guide ?? [],
    children: childRows.map(publicSession),
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

  // The derived title in SQL: title ?? action name ?? agent name ?? the
  // first issue's title ?? the first person message's first line. Columns
  // are spelled qualified: drizzle renders select-field columns bare, which
  // the subqueries' own `id`s would make ambiguous.
  const derivedTitle = sql<string>`coalesce(
    nullif(trim(${sessions.title}), ''),
    nullif(trim(${sessions.actionName}), ''),
    nullif(trim(${sessions.name}), ''),
    (SELECT i.title FROM session_issues si JOIN issues i ON i.id = si.issue_id
      WHERE si.session_id = "sessions"."id" ORDER BY si.sort_order, si.created_at LIMIT 1),
    (SELECT nullif(trim(split_part(m.body, E'\\n', 1)), '') FROM messages m
      WHERE m.session_id = "sessions"."id" AND m.author_kind = 'user'
      ORDER BY m.created_at LIMIT 1),
    'Untitled'
  )`

  const rows = await db
    .select({
      id: sessions.id,
      teamId: sessions.teamId,
      kind: sessions.kind,
      title: derivedTitle,
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
  return rows
}
