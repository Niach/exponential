import { useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  messageCollection,
  pullRequestCollection,
  pullRequestIssueCollection,
  sessionCollection,
  sessionIssueCollection,
} from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { teamScopeIds } from "@/lib/team-scope"
import {
  buildSessionThread,
  listAgents,
  partitionThreads,
  sessionCard,
  toTreeIssue,
  toTreeMessage,
  toTreePullRequest,
  toTreeSession,
  toTreeSessionIssue,
  treeOrder,
  type AgentRow,
  type MessageRow,
  type PullRequestRow,
  type SessionCard,
  type SessionIssueRow,
  type SessionRow,
  type SessionThreadView,
  type ThreadsListGroup,
  type TreeIssue,
  type TreeSession,
} from "@/lib/session-tree"

// EXP-1293 Threads v2 (web): ONE read layer over the session-tree shapes.
// Every screen (Threads, the rail's Agents + Running, a thread, an agent's
// channel, a session, an issue's Threads row) reads through these, and the
// view models are the contract's (fixture-locked ×4).

/** Every synced session of the team(s) (roots + children), raw + tree form. */
export function useTeamSessions(teamId: string | readonly string[] | null | undefined) {
  const teamIds = teamScopeIds(teamId ?? undefined)
  const key = teamIds.join(`,`)
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) => inArray(sessions.teamId, teamIds))
        : undefined,
    [key]
  )
  return useMemo(() => {
    const rows = (data ?? []) as SessionRow[]
    return { rows, tree: rows.map(toTreeSession), isReady: teamIds.length > 0 && isReady }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, isReady, key])
}

/** EXP-1316: a thread title may fall back past its issues (to the stored
 *  name, the first message) only once the shapes that could NAME an issue
 *  have reported ready: the links, then the linked issues (none linked = no
 *  wait), and the messages. Until then hosts hold the title (and the
 *  chips), so nothing flips from the request text to `Implement EXP-12`. */
export function titleShapesReady(input: {
  linksReady: boolean
  issueCount: number
  issuesReady: boolean
  messagesReady: boolean
}): boolean {
  return (
    input.linksReady && (input.issueCount === 0 || input.issuesReady) && input.messagesReady
  )
}

/** Hold every row title (empty) until the title shapes are ready. */
export function holdTitles(groups: ThreadsListGroup[], ready: boolean): ThreadsListGroup[] {
  if (ready) return groups
  return groups.map((group) => ({
    ...group,
    rows: group.rows.map((row) => ({ ...row, title: `` })),
  }))
}

/** The issue + message context `threadTitle` needs for a set of roots. */
function useTitleContext(teamIds: readonly string[]) {
  const key = teamIds.join(`,`)
  const { data: links, isReady: linksReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query
            .from({ links: sessionIssueCollection })
            .where(({ links }) => inArray(links.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  // The issues shape never streams its `team_id` scope: resolve the linked
  // ids instead.
  const issueIds = [...new Set(((links ?? []) as SessionIssueRow[]).map((link) => link.issueId))]
  const issueKey = issueIds.sort().join(`,`)
  const { data: issues, isReady: issuesReady } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
            .select(({ issues }) => ({
              id: issues.id,
              identifier: issues.identifier,
              title: issues.title,
            }))
        : undefined,
    [issueKey]
  )
  const { data: messages, isReady: messagesReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => inArray(messages.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  const ready = titleShapesReady({
    linksReady: teamIds.length > 0 && linksReady,
    issueCount: issueIds.length,
    issuesReady,
    messagesReady,
  })
  return useMemo(
    () => ({
      sessionIssues: ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue),
      issues: ((issues ?? []) as TreeIssue[]).map(toTreeIssue),
      messages: ((messages ?? []) as MessageRow[]).map(toTreeMessage),
      ready,
    }),
    [links, issues, messages, ready]
  )
}

/**
 * The Threads list (Needs you · Running · Recent) for the ACTIVE team
 * (`teamId`; null/array = every listed team, phones band per team).
 */
export function useThreadsList(teamId: string | readonly string[] | null | undefined): {
  groups: ThreadsListGroup[]
  isReady: boolean
} {
  const teamIds = teamScopeIds(teamId ?? undefined)
  const { tree, isReady } = useTeamSessions(teamIds)
  const context = useTitleContext(teamIds)
  return useMemo(() => {
    const groups = partitionThreads({
      sessions: tree,
      sessionIssues: context.sessionIssues,
      issues: context.issues,
      messages: context.messages,
      teamId: null,
    })
    // EXP-1316: rows hold their title until the issue shapes are ready.
    return { groups: holdTitles(groups, context.ready), isReady: isReady && context.ready }
  }, [tree, context, isReady])
}

/** The rail's Agents section: the team's agent roots, like boards. */
export function useAgents(teamId: string | readonly string[] | null | undefined): {
  agents: AgentRow[]
  isReady: boolean
} {
  const { tree, isReady } = useTeamSessions(teamId)
  return useMemo(() => ({ agents: listAgents(tree, null), isReady }), [tree, isReady])
}

/** One synced session row (any node of any tree). */
export function useSessionRow(sessionId: string | null | undefined): {
  row: SessionRow | null
  isReady: boolean
} {
  const { data, isReady } = useLiveQuery(
    (query) =>
      sessionId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) => eq(sessions.id, sessionId))
        : undefined,
    [sessionId]
  )
  const row = ((data ?? []) as SessionRow[])[0] ?? null
  return { row, isReady: Boolean(sessionId) && isReady }
}

/** Every session of one tree (root + descendants), creation order. */
export function useTreeSessions(rootId: string | null | undefined) {
  const { data, isReady } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) => eq(sessions.rootId, rootId))
        : undefined,
    [rootId]
  )
  const { data: self } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) => eq(sessions.id, rootId))
        : undefined,
    [rootId]
  )
  return useMemo(() => {
    const byId = new Map<string, SessionRow>()
    for (const row of [...((self ?? []) as SessionRow[]), ...((data ?? []) as SessionRow[])])
      byId.set(row.id, row)
    const rows = [...byId.values()]
    return { rows, tree: treeOrder(rows.map(toTreeSession)), isReady: Boolean(rootId) && isReady }
  }, [data, self, isReady, rootId])
}

/** The PRs a thread opened (`root_session_id`). */
export function useRootPullRequests(rootId: string | null | undefined): PullRequestRow[] {
  const { data } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ prs: pullRequestCollection })
            .where(({ prs }) => eq(prs.rootSessionId, rootId))
        : undefined,
    [rootId]
  )
  return useMemo(
    () =>
      ((data ?? []) as PullRequestRow[]).sort((a, b) => a.number - b.number || (a.id < b.id ? -1 : 1)),
    [data]
  )
}

/** The PRs linked to one issue (`pull_request_issues`). */
export function useIssuePullRequests(issueId: string | null | undefined): PullRequestRow[] {
  const { data } = useLiveQuery(
    (query) =>
      issueId
        ? query
            .from({ links: pullRequestIssueCollection })
            .where(({ links }) => eq(links.issueId, issueId))
            .innerJoin({ prs: pullRequestCollection }, ({ links, prs }) =>
              eq(links.pullRequestId, prs.id)
            )
            .select(({ prs }) => ({ ...prs }))
        : undefined,
    [issueId]
  )
  return useMemo(
    () => ((data ?? []) as PullRequestRow[]).sort((a, b) => b.number - a.number),
    [data]
  )
}

/** The thread screen's whole model (`buildSessionThread`, fixture-locked). */
export function useSessionThread(
  rootId: string | null | undefined,
  /** EXP-1316: the session open in place (its chips = that level). */
  focus: string | null = null
): {
  view: SessionThreadView | null
  root: SessionRow | null
  rows: SessionRow[]
  pullRequests: PullRequestRow[]
  isReady: boolean
  /** EXP-1316: the issue shapes reported ready (the title cannot flip). */
  titleReady: boolean
} {
  const { rows, tree, isReady } = useTreeSessions(rootId)
  const root = rows.find((row) => row.id === rootId) ?? null
  const { data: links, isReady: linksReady } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ links: sessionIssueCollection })
            .where(({ links }) => eq(links.sessionId, rootId))
        : undefined,
    [rootId]
  )
  const issueIds = ((links ?? []) as SessionIssueRow[]).map((link) => link.issueId)
  const issueKey = [...issueIds].sort().join(`,`)
  const { data: issues, isReady: issuesReady } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
            .select(({ issues }) => ({
              id: issues.id,
              identifier: issues.identifier,
              title: issues.title,
            }))
        : undefined,
    [issueKey]
  )
  const { data: messages, isReady: messagesReady } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => eq(messages.sessionId, rootId))
        : undefined,
    [rootId]
  )
  const titleReady = titleShapesReady({
    linksReady: Boolean(rootId) && linksReady,
    issueCount: issueIds.length,
    issuesReady,
    messagesReady,
  })
  const pullRequests = useRootPullRequests(rootId)
  const now = useNow(1_000)
  const view = useMemo(() => {
    if (!root) return null
    return buildSessionThread({
      root: toTreeSession(root),
      sessions: tree,
      sessionIssues: ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue),
      issues: ((issues ?? []) as TreeIssue[]).map(toTreeIssue),
      messages: ((messages ?? []) as MessageRow[]).map(toTreeMessage),
      pullRequests: pullRequests.map(toTreePullRequest),
      now: now.toISOString(),
    }, { focus })
  }, [root, tree, links, issues, messages, pullRequests, now, focus])
  return { view, root, rows, pullRequests, isReady, titleReady }
}

/** The messages of ONE session (an agent's channel, an opened child). */
export function useSessionMessages(sessionId: string | null | undefined): MessageRow[] {
  const { data } = useLiveQuery(
    (query) =>
      sessionId
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => eq(messages.sessionId, sessionId))
        : undefined,
    [sessionId]
  )
  return useMemo(
    () =>
      ((data ?? []) as MessageRow[]).sort(
        (a, b) =>
          new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime() ||
          (a.id < b.id ? -1 : 1)
      ),
    [data]
  )
}

/** THE session card for any session row (ordinal = its tree position). */
export function useSessionCard(
  row: SessionRow | null,
  tree: readonly TreeSession[]
): SessionCard | null {
  const now = useNow(1_000)
  return useMemo(() => {
    if (!row) return null
    const index = tree.findIndex((session) => session.id === row.id)
    return sessionCard(toTreeSession(row), index >= 0 ? index + 1 : 1, now.toISOString())
  }, [row, tree, now])
}

/** The threads (roots) one issue sits in, newest activity first. */
export function useThreadsOfIssue(issueId: string | null | undefined): SessionRow[] {
  const { data } = useLiveQuery(
    (query) =>
      issueId
        ? query
            .from({ links: sessionIssueCollection })
            .where(({ links }) => eq(links.issueId, issueId))
            .innerJoin({ sessions: sessionCollection }, ({ links, sessions }) =>
              eq(links.sessionId, sessions.id)
            )
            .select(({ sessions }) => ({ ...sessions }))
        : undefined,
    [issueId]
  )
  return useMemo(
    () =>
      ((data ?? []) as SessionRow[]).sort(
        (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()
      ),
    [data]
  )
}
