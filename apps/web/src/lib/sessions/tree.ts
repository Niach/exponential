import { and, asc, desc, eq, inArray, isNull, ne, or } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  devices,
  issues,
  messages,
  pullRequests,
  sessionIssues,
  sessions,
  type DeviceAgentHealth,
  type Session,
} from "@/db/schema"
import { getUserTeamIds } from "@/lib/auth/membership"
import { loadSessionForUser } from "@/lib/sessions/access"
import { deriveSessionTitles } from "@/lib/sessions/title"

// EXP-1291/NEW-1/NEW-2: the session tree as one read (MCP `sessions_get`)
// and the thread list (MCP `sessions_list`: thread roots; an agent's
// conversation sessions are listed through `agents_list`). Same visibility
// rule as the shapes; the server-only facets (`mcp_server_ids`, `wake_on`)
// never leave here. A root's issues = the UNION over its tree's rows (each
// row names the session holding it).

export type PublicSession = Omit<Session, `mcpServerIds` | `wakeOn`>

export function publicSession(session: Session): PublicSession {
  const { mcpServerIds: _mcp, wakeOn: _wake, ...rest } = session
  return rest
}

/** #FEED-84: the login a session runs on, as its HOST device reports it
 *  (`devices.agent_accounts.<agent>.profiles[]`, the rows devices_list
 *  returns): `accountEmail` + `accountHealth`, null when unknown. */
export interface SessionAccountView {
  accountEmail: string | null
  accountHealth: DeviceAgentHealth | null
}

type AccountKey = Pick<Session, `deviceId` | `userId` | `teamId` | `cli` | `account`>

export async function resolveSessionAccounts(
  rows: readonly (AccountKey & { id: string })[]
): Promise<Map<string, SessionAccountView>> {
  const out = new Map<string, SessionAccountView>()
  const wanted = rows.filter((row) => row.account)
  const deviceIds = [...new Set(wanted.map((row) => row.deviceId))]
  const deviceRows = deviceIds.length
    ? await db
        .select({
          deviceId: devices.deviceId,
          userId: devices.userId,
          sharedTeamIds: devices.sharedTeamIds,
          agentAccounts: devices.agentAccounts,
          launchDefaults: devices.launchDefaults,
        })
        .from(devices)
        .where(inArray(devices.deviceId, deviceIds))
    : []
  for (const row of rows) {
    // NEW-1: the host = the starter's own row, else a row sharing the team
    // (the same rule as `lib/sessions/host.ts`).
    const machine = deviceRows.filter((entry) => entry.deviceId === row.deviceId)
    const device =
      machine.find((entry) => entry.userId === row.userId) ??
      machine
        .filter((entry) => (entry.sharedTeamIds ?? []).includes(row.teamId))
        .sort((a, b) => (a.userId < b.userId ? -1 : 1))[0]
    const agent = row.cli ?? device?.launchDefaults?.defaultAgent ?? null
    const accounts = device?.agentAccounts ?? {}
    const agents = agent ? [agent] : Object.keys(accounts)
    const profile = row.account
      ? agents
          .flatMap((key) => accounts[key]?.profiles ?? [])
          .find((entry) => entry?.id === row.account)
      : undefined
    out.set(row.id, {
      accountEmail: profile?.email || null,
      accountHealth: profile
        ? (profile.health ?? (profile.signedIn ? `ok` : `signed_out`))
        : null,
    })
  }
  return out
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
    // The union over the tree: each row names the session holding it.
    db
      .select({
        issueId: sessionIssues.issueId,
        identifier: issues.identifier,
        title: issues.title,
        status: issues.status,
        source: sessionIssues.source,
        sessionId: sessionIssues.sessionId,
        addedBySessionId: sessionIssues.addedBySessionId,
      })
      .from(sessionIssues)
      .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
      .innerJoin(sessions, eq(sessions.id, sessionIssues.sessionId))
      .where(
        and(
          eq(sessions.rootId, root.id),
          isNull(sessionIssues.boardDeletedAt),
          isNull(sessionIssues.boardArchivedAt)
        )
      )
      .orderBy(asc(sessions.depth), asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt)),
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
  const accounts = await resolveSessionAccounts([root, ...childRows])
  return {
    root: { ...publicSession(root), displayTitle: titles.get(root.id)!, ...accounts.get(root.id)! },
    issues: issueRows,
    pullRequests: prRows,
    guide: root.guide ?? [],
    children: childRows.map((child) => ({
      ...publicSession(child),
      displayTitle: titles.get(child.id)!,
      ...accounts.get(child.id)!,
    })),
    // Oldest first, the last `limit` of the whole tree.
    messages: messageRows.reverse(),
  }
}

export interface SessionListRow {
  id: string
  teamId: string
  /** NEW-2: the agent this is a conversation of (null = a thread). */
  agentId: string | null
  title: string
  status: Session[`status`]
  busy: boolean
  needsInput: boolean
  deviceId: string
  userId: string
  shared: boolean
  // #FEED-84: the runner and its login (`accountEmail`/`accountHealth` off
  // the host device's report), and the usage wall.
  cli: string | null
  account: string | null
  accountEmail: string | null
  accountHealth: DeviceAgentHealth | null
  blocked: Session[`blocked`]
  name: string | null
  actionName: string | null
  createdAt: Date
  updatedAt: Date
}

/** Roots the caller may see, most recently active first: threads, or with
 *  `agentId` the caller's conversations with that agent. `live` = not
 *  ended. */
export async function listSessions(
  userId: string,
  args: { teamId?: string; agentId?: string; live?: boolean; limit: number; offset: number }
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
      agentId: sessions.agentId,
      rootId: sessions.rootId,
      title: sessions.title,
      status: sessions.status,
      busy: sessions.busy,
      needsInput: sessions.needsInput,
      deviceId: sessions.deviceId,
      userId: sessions.userId,
      shared: sessions.shared,
      cli: sessions.cli,
      account: sessions.account,
      blocked: sessions.blocked,
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
        // NEW-2: `agentId` = the caller's OWN conversations with that agent.
        args.agentId
          ? and(eq(sessions.agentId, args.agentId), eq(sessions.userId, userId))
          : isNull(sessions.agentId),
        or(eq(sessions.shared, true), eq(sessions.userId, userId)),
        ...(args.live ? [ne(sessions.status, `ended`)] : [])
      )
    )
    .orderBy(desc(sessions.updatedAt))
    .limit(args.limit)
    .offset(args.offset)
  // EXP-1341: THE title chain (session-title.json), never a SQL copy.
  const titles = await deriveSessionTitles(rows)
  const accounts = await resolveSessionAccounts(rows)
  return rows.map(({ rootId: _root, ...row }) => ({
    ...row,
    title: titles.get(row.id)!,
    ...accounts.get(row.id)!,
  }))
}
