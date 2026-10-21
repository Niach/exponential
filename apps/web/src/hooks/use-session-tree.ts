import { useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueRelationCollection,
  messageCollection,
  pullRequestCollection,
  pullRequestIssueCollection,
  sessionCollection,
  sessionIssueCollection,
  sessionReadsCollection,
  agentCollection,
} from "@/lib/collections"
import { sessionTitles } from "@exp/domain-contract/session-title"
import { useNow } from "@/hooks/use-now"
import { teamScopeIds } from "@/lib/team-scope"
import {
  buildSessionThread,
  listAgents,
  type AgentLike,
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
  type TreeSessionIssue,
  type TreeSessionRead,
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

/** EXP-1316 / EXP-1341: the sessions whose title (`sessionTitle`) could
 *  still flip while the title shapes load: only those are held. A name or
 *  a stored title never flips; otherwise the links decide whether the
 *  session has OWN issues (a root's links, a child's assigned ones), which
 *  then wait for the issues; with none, an action names it; with no
 *  action it waits for its first prompt (the messages). */
export function unsettledTitleRoots(input: {
  sessions: readonly Pick<TreeSession, `id` | `rootId` | `name` | `actionName` | `title`>[]
  sessionIssues: readonly Pick<TreeSessionIssue, `sessionId`>[]
  linksReady: boolean
  issuesReady: boolean
  messagesReady: boolean
}): Set<string> {
  // NEW-1: a session's own issues = the rows it holds (root or child alike).
  const holding = new Set(input.sessionIssues.map((link) => link.sessionId))
  const held = new Set<string>()
  for (const session of input.sessions) {
    if (session.name?.trim() || session.title?.trim()) continue
    if (!input.linksReady) {
      held.add(session.id)
      continue
    }
    const ownIssues = holding.has(session.id)
    if (ownIssues) {
      if (!input.issuesReady) held.add(session.id)
      continue
    }
    if (session.actionName?.trim()) continue
    if (!input.messagesReady) held.add(session.id)
  }
  return held
}

/** Blank the title of the rows in `held` (their ladder still waits on a
 *  shape); every other row keeps its title. */
export function holdTitles(
  groups: ThreadsListGroup[],
  held: ReadonlySet<string>
): ThreadsListGroup[] {
  if (held.size === 0) return groups
  return groups.map((group) => ({
    ...group,
    rows: group.rows.map((row) => (held.has(row.sessionId) ? { ...row, title: `` } : row)),
  }))
}

/** The issue + message context `sessionTitle` needs for a team's sessions. */
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
  const scopedLinksReady = teamIds.length > 0 && linksReady
  const ready = titleShapesReady({
    linksReady: scopedLinksReady,
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
      linksReady: scopedLinksReady,
      issuesReady,
      messagesReady,
    }),
    [links, issues, messages, ready, scopedLinksReady, issuesReady, messagesReady]
  )
}

/** EXP-1323: the viewer's read marks (`session_reads`, a per-user shape). */
export function useSessionReads(): TreeSessionRead[] {
  const { data } = useLiveQuery((query) => query.from({ reads: sessionReadsCollection }), [])
  return useMemo(
    () =>
      ((data ?? []) as { sessionId: string; readAt: Date | string }[]).map((read) => ({
        sessionId: read.sessionId,
        readAt: read.readAt instanceof Date ? read.readAt.toISOString() : read.readAt,
      })),
    [data]
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
  const reads = useSessionReads()
  return useMemo(() => {
    const groups = partitionThreads({
      sessions: tree,
      sessionIssues: context.sessionIssues,
      issues: context.issues,
      messages: context.messages,
      teamId: null,
      reads,
    })
    // EXP-1316: a row holds its title only while ITS ladder still waits on
    // a shape (an issue-linked row until the issues land); a row named by
    // its agent, action or stored title shows at once.
    const held = unsettledTitleRoots({
      sessions: tree,
      sessionIssues: context.sessionIssues,
      linksReady: context.linksReady,
      issuesReady: context.issuesReady,
      messagesReady: context.messagesReady,
    })
    return { groups: holdTitles(groups, held), isReady: isReady && held.size === 0 }
  }, [tree, context, isReady, reads])
}

/** NEW-2: the rail's Agents section: the team's `agents` definitions, like
 *  boards; busy / needs-you from the viewer's own conversation with each. */
export function useAgents(
  teamId: string | readonly string[] | null | undefined,
  me: string | null | undefined
): {
  agents: AgentRow[]
  isReady: boolean
} {
  const teamIds = teamScopeIds(teamId ?? undefined)
  const key = teamIds.join(`,`)
  const { data: agentRows, isReady: agentsReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query.from({ agents: agentCollection }).where(({ agents }) => inArray(agents.teamId, teamIds))
        : undefined,
    [key]
  )
  const { tree, isReady } = useTeamSessions(teamId)
  return useMemo(
    () => ({
      agents: listAgents((agentRows ?? []) as AgentLike[], tree, null, me ?? null),
      isReady: isReady && (teamIds.length === 0 || agentsReady),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [agentRows, tree, isReady, agentsReady, key, me]
  )
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

interface ParentRelation {
  type: string
  /** The PARENT (EXP-736 canonical direction). */
  issueId: string
  /** The CHILD. */
  relatedIssueId: string
}

/** EXP-1334: each linked issue's `parentId` = its nearest LINKED ancestor
 *  over the team's `parent` relations (walking past unlinked ones; none
 *  = null; a linked cycle is handed on, the contract folds it). Several
 *  parents: the first by id, deterministic. */
export function linkedIssueParents(
  issues: readonly TreeIssue[],
  relations: readonly ParentRelation[]
): TreeIssue[] {
  const parentOf = new Map<string, string>()
  for (const relation of [...relations].sort((a, b) =>
    a.issueId < b.issueId ? -1 : a.issueId > b.issueId ? 1 : 0
  )) {
    if (relation.type !== `parent` || relation.issueId === relation.relatedIssueId) continue
    if (!parentOf.has(relation.relatedIssueId)) {
      parentOf.set(relation.relatedIssueId, relation.issueId)
    }
  }
  const linked = new Set(issues.map((issue) => issue.id))
  const linkedParent = new Map<string, string>()
  for (const issue of issues) {
    const seen = new Set([issue.id])
    let cursor = parentOf.get(issue.id)
    while (cursor && !seen.has(cursor)) {
      if (linked.has(cursor)) {
        linkedParent.set(issue.id, cursor)
        break
      }
      seen.add(cursor)
      cursor = parentOf.get(cursor)
    }
  }
  return issues.map((issue) => ({
    ...issue,
    // A linked-parent cycle is the contract's (`foldForest` promotes its
    // first unreached issue to a chip).
    parentId: linkedParent.get(issue.id) ?? null,
  }))
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
  /** EXP-1341: every tree session's `sessionTitle`, off the SAME inputs. */
  titles: Map<string, string>
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
  // EXP-1334: the WHOLE tree's messages (`root_id`): the root's own rows
  // plus its children's question / plan cards, which surface in the
  // Conversation (`buildSessionThread` picks them).
  const { data: messages, isReady: messagesReady } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => eq(messages.rootId, rootId))
        : undefined,
    [rootId]
  )
  // EXP-1334: the issue tree (`issue_relations` kind parent): a linked
  // sub-issue folds under its nearest LINKED ancestor. The walk passes
  // UNLINKED ancestors (`linkedIssueParents`), so the subscription is the
  // team's whole `parent` set (a filter on the linked ids would cut the
  // chain); what the view keys on is the FOLD below, which keeps its
  // identity while no linked issue's parent moved, so a relation elsewhere
  // in the team never rebuilds the thread.
  const { data: parentRelations } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ relations: issueRelationCollection })
            .where(({ relations }) => eq(relations.type, `parent`))
        : undefined,
    [issueIds.length > 0]
  )
  const foldedIssues = useFoldedIssues(
    (issues ?? []) as TreeIssue[],
    (parentRelations ?? []) as ParentRelation[]
  )
  const titleReady = titleShapesReady({
    linksReady: Boolean(rootId) && linksReady,
    issueCount: issueIds.length,
    issuesReady,
    messagesReady,
  })
  const pullRequests = useRootPullRequests(rootId)
  const titles = useMemo(
    () =>
      sessionTitles(
        tree,
        ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue),
        foldedIssues,
        ((messages ?? []) as MessageRow[]).map(toTreeMessage)
      ),
    [tree, links, foldedIssues, messages]
  )
  const now = useNow(1_000)
  const view = useMemo(() => {
    if (!root) return null
    return buildSessionThread({
      root: toTreeSession(root),
      sessions: tree,
      sessionIssues: ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue),
      issues: foldedIssues,
      messages: ((messages ?? []) as MessageRow[]).map(toTreeMessage),
      pullRequests: pullRequests.map(toTreePullRequest),
      now: now.toISOString(),
    }, { focus })
  }, [root, tree, links, foldedIssues, messages, pullRequests, now, focus])
  return { view, root, rows, pullRequests, isReady, titleReady, titles }
}

/** The linked issues with their fold parents (`linkedIssueParents`), the
 *  SAME array while nothing a chip reads changed (id, identifier, title,
 *  parent): the relations shape moves on any relation of the team. */
function useFoldedIssues(
  issues: readonly TreeIssue[],
  relations: readonly ParentRelation[]
): TreeIssue[] {
  const folded = useMemo(
    () => linkedIssueParents(issues.map(toTreeIssue), relations),
    [issues, relations]
  )
  const key = folded
    .map((issue) => [issue.id, issue.identifier, issue.title, issue.parentId ?? ``].join(`\u0001`))
    .join(`\u0002`)
  // Keyed on the fold's content, not its identity.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => folded, [key])
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

/** EXP-1341: THE display title (`sessionTitles`, fixture
 *  `session-title.json`) of any set of session rows: their roots' issue
 *  links, the linked issues, and the rows' own messages, read in three live
 *  queries for the whole set. `held` = the rows whose title could still
 *  flip (`unsettledTitleRoots`); `titles` omits them, `ready` = none held. */
export function useSessionTitles(rows: readonly SessionRow[]): {
  titles: Map<string, string>
  held: Set<string>
  ready: boolean
} {
  const ids = [...new Set(rows.map((row) => row.id))].sort()
  const rootIds = [...new Set(rows.map((row) => row.rootId ?? row.id))].sort()
  const key = ids.join(`,`)
  const rootKey = rootIds.join(`,`)
  const { data: links, isReady: linksReady } = useLiveQuery(
    (query) =>
      rootIds.length > 0
        ? query
            .from({ links: sessionIssueCollection })
            .where(({ links }) => inArray(links.sessionId, rootIds))
        : undefined,
    [rootKey]
  )
  const issueIds = [
    ...new Set(((links ?? []) as SessionIssueRow[]).map((link) => link.issueId)),
  ].sort()
  const issueKey = issueIds.join(`,`)
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
      ids.length > 0
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => inArray(messages.sessionId, ids))
        : undefined,
    [key]
  )
  return useMemo(() => {
    const sessions = rows.map(toTreeSession)
    const sessionIssues = ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue)
    const held = unsettledTitleRoots({
      sessions,
      sessionIssues,
      linksReady: rootIds.length > 0 && linksReady,
      issuesReady,
      messagesReady: ids.length > 0 && messagesReady,
    })
    const titles = sessionTitles(
      sessions.filter((session) => !held.has(session.id)),
      sessionIssues,
      ((issues ?? []) as TreeIssue[]).map(toTreeIssue),
      ((messages ?? []) as MessageRow[]).map(toTreeMessage)
    )
    return { titles, held, ready: held.size === 0 }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, links, issues, messages, linksReady, issuesReady, messagesReady])
}

/** THE session card for any session row (`sessionCard` over its
 *  `sessionTitle`); null while the row is absent or its title is held. */
export function useSessionCard(row: SessionRow | null): SessionCard | null {
  const now = useNow(1_000)
  const rows = useMemo(() => (row ? [row] : []), [row])
  const { titles } = useSessionTitles(rows)
  return useMemo(() => {
    if (!row) return null
    const title = titles.get(row.id)
    if (title === undefined) return null
    return sessionCard(toTreeSession(row), title, now.toISOString())
  }, [row, titles, now])
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
