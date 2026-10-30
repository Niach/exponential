import { useMemo, useSyncExternalStore } from "react"
import { openDigestMessages } from "@/lib/enet/sealed-open"
import { useKeyVersion } from "@/lib/enet/use-key-holder"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  conversationCollection,
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
import { CHATS_TRANSCRIPT, digestMessages, type ConversationDigest } from "@exp/domain-contract/chats"
import { sessionWaitingCard } from "@exp/domain-contract/thread-row-action"
import { useNow } from "@/hooks/use-now"
import { teamScopeIds } from "@/lib/team-scope"
import {
  buildSessionThread,
  conversationSessions,
  keyMessageRows,
  listAgents,
  type AgentLike,
  partitionThreads,
  sessionCard,
  sessionConversationIds,
  threadAskRow,
  type ThreadRow,
  toTreeIssue,
  toTreeMessages,
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
  type SyncedMessageRow,
  type ThreadsListGroup,
  type TreeIssue,
  type TreeMessage,
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
  // EXP-1459: titles re-open when a tree key arrives.
  const keyVersion = useKeyVersion()
  return useMemo(() => {
    const rows = (data ?? []) as SessionRow[]
    return { rows, tree: rows.map(toTreeSession), isReady: teamIds.length > 0 && isReady }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, isReady, key, keyVersion])
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

/** NEW-8: conversation id → session id over the synced `conversations`
 *  (`conversationSessions`): every message is keyed through it
 *  (`keyMessageRows`), never through its own legacy `sessionId`. Anything
 *  waiting on the messages waits on `isReady` too. */
export function useConversationSessions(): {
  conversations: Map<string, string>
  isReady: boolean
} {
  const { data, isReady } = useLiveQuery(
    (query) =>
      query.from({ conversations: conversationCollection }).select(({ conversations }) => ({
        id: conversations.id,
        sessionId: conversations.sessionId,
      })),
    []
  )
  const conversations = useMemo(
    () =>
      conversationSessions((data ?? []) as { id: string; sessionId: string | null }[]),
    [data]
  )
  return { conversations, isReady }
}

/** #EXP-1380: the messages the LIST reads (titles, previews, unread,
 *  rings) for the sessions of `teamIds` or of `sessionIds`, out of each
 *  conversation's server-kept digest (`digestMessages`): the list never
 *  syncs a transcript, a chat's messages load when it opens. */
export function useDigestMessages(scope: {
  teamIds?: readonly string[]
  sessionIds?: readonly string[]
}): { messages: TreeMessage[]; isReady: boolean } {
  const teamIds = scope.teamIds ?? null
  const sessionIds = scope.sessionIds ?? null
  const key = `${teamIds?.join(`,`) ?? ``}|${sessionIds?.join(`,`) ?? ``}`
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamIds && teamIds.length > 0
        ? query
            .from({ conversations: conversationCollection })
            .where(({ conversations }) => inArray(conversations.teamId, [...teamIds]))
            .select(({ conversations }) => ({
              id: conversations.id,
              sessionId: conversations.sessionId,
              digest: conversations.digest,
            }))
        : sessionIds && sessionIds.length > 0
          ? query
              .from({ conversations: conversationCollection })
              .where(({ conversations }) => inArray(conversations.sessionId, [...sessionIds]))
              .select(({ conversations }) => ({
                id: conversations.id,
                sessionId: conversations.sessionId,
                digest: conversations.digest,
              }))
          : undefined,
    [key]
  )
  // EXP-1459: digest bodies/cards are sealed like their messages: open them
  // with the session's ROOT keys (re-run when a key arrives).
  const keyVersion = useKeyVersion()
  const messages = useMemo(
    () =>
      ((data ?? []) as { sessionId: string | null; digest: unknown }[]).flatMap((row) =>
        row.sessionId
          ? openDigestMessages(
              row.sessionId,
              digestMessages(row.sessionId, row.digest as ConversationDigest | null),
              rootOfSession
            )
          : []
      ),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data, keyVersion]
  )
  return { messages, isReady }
}

/** EXP-1459: a synced session's root (a row not synced yet = itself). */
function rootOfSession(sessionId: string): string {
  return (sessionCollection.get(sessionId) as SessionRow | undefined)?.rootId ?? sessionId
}

/** NEW-8: the sessions among `sessionIds` whose own conversation has not
 *  synced yet. Every session owns one from its insert
 *  (`create_session_conversation`), so a missing row is the conversations
 *  stream lagging (a brand-new session), never "no messages": their
 *  messages are NOT ready (a disabled live query reports ready, empty). */
export function sessionsAwaitingConversation(
  conversations: ReadonlyMap<string, string>,
  sessionIds: Iterable<string>
): Set<string> {
  const pending = new Set(sessionIds)
  for (const sessionId of conversations.values()) pending.delete(sessionId)
  return pending
}

/** EXP-1316 / EXP-1341: the sessions whose title (`sessionTitle`) could
 *  still flip while the title shapes load: only those are held. A name or
 *  a stored title never flips; otherwise the links decide whether the
 *  session has OWN issues (a root's links, a child's assigned ones), which
 *  then wait for the issues; with none it waits for its first prompt (the
 *  messages). */
export function unsettledTitleRoots(input: {
  sessions: readonly Pick<TreeSession, `id` | `rootId` | `title`>[]
  sessionIssues: readonly Pick<TreeSessionIssue, `sessionId`>[]
  linksReady: boolean
  issuesReady: boolean
  messagesReady: boolean
  /** NEW-8: sessions whose conversation has not synced: their messages
   *  are not ready even when the shapes are (`sessionsAwaitingConversation`). */
  awaitingConversation?: ReadonlySet<string>
}): Set<string> {
  // NEW-1: a session's own issues = the rows it holds (root or child alike).
  const holding = new Set(input.sessionIssues.map((link) => link.sessionId))
  const held = new Set<string>()
  for (const session of input.sessions) {
    if (session.title?.trim()) continue
    if (!input.linksReady) {
      held.add(session.id)
      continue
    }
    const ownIssues = holding.has(session.id)
    if (ownIssues) {
      if (!input.issuesReady) held.add(session.id)
      continue
    }
    if (!input.messagesReady || input.awaitingConversation?.has(session.id)) held.add(session.id)
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
  // #EXP-1380: the digests, never the transcripts (`useDigestMessages`).
  const { messages, isReady: digestsReady } = useDigestMessages({ teamIds })
  const { conversations, isReady: conversationsReady } = useConversationSessions()
  const messagesReady = digestsReady && conversationsReady
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
      messages,
      ready,
      linksReady: scopedLinksReady,
      issuesReady,
      messagesReady,
      conversations,
    }),
    [links, issues, messages, ready, scopedLinksReady, issuesReady, messagesReady, conversations]
  )
}

/** EXP-1323: the viewer's read marks (`session_reads`, a per-user shape). */
export function useSessionReads(): TreeSessionRead[] {
  const { data } = useLiveQuery((query) => query.from({ reads: sessionReadsCollection }), [])
  return useMemo(
    () =>
      (
        (data ?? []) as {
          sessionId: string
          readAt: Date | string
          archivedAt?: Date | string | null
        }[]
      ).map((read) => ({
        sessionId: read.sessionId,
        readAt: read.readAt instanceof Date ? read.readAt.toISOString() : read.readAt,
        // #EXP-1388: the viewer's archive of the chat (chatList's `view`).
        archivedAt:
          read.archivedAt instanceof Date ? read.archivedAt.toISOString() : (read.archivedAt ?? null),
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
  // NEW-1: Recent = the last `recent.hours`, then the last `recent.max`;
  // a minute's grain is enough for a 24 h window.
  const now = useNow(60_000)
  const nowIso = now.toISOString()
  return useMemo(() => {
    const groups = partitionThreads({
      sessions: tree,
      sessionIssues: context.sessionIssues,
      issues: context.issues,
      messages: context.messages,
      teamId: null,
      reads,
      now: nowIso,
    })
    // EXP-1316: a row holds its title only while ITS ladder still waits on
    // a shape (an issue-linked row until the issues land); a row with a
    // stored title shows at once.
    const held = unsettledTitleRoots({
      sessions: tree,
      sessionIssues: context.sessionIssues,
      linksReady: context.linksReady,
      issuesReady: context.issuesReady,
      messagesReady: context.messagesReady,
      awaitingConversation: sessionsAwaitingConversation(
        context.conversations,
        tree.map((session) => session.id)
      ),
    })
    return { groups: holdTitles(groups, held), isReady: isReady && held.size === 0 }
  }, [tree, context, isReady, reads, nowIso])
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
  const keyVersion = useKeyVersion()
  return useMemo(() => {
    const byId = new Map<string, SessionRow>()
    for (const row of [...((self ?? []) as SessionRow[]), ...((data ?? []) as SessionRow[])])
      byId.set(row.id, row)
    const rows = [...byId.values()]
    return { rows, tree: treeOrder(rows.map(toTreeSession)), isReady: Boolean(rootId) && isReady }
  }, [data, self, isReady, rootId, keyVersion])
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

// ── #EXP-1380: a chat loads its NEWEST messages only ────────────────────
// (chats.json WINDOW): `transcript.window + windowStep` of them per chat, one
// shared limit per chat key (its root, or the session of a lone
// conversation) so every query of the chat asks the SAME subset; `Load
// earlier` raises it by `windowStep` (`loadEarlierMessages`). The WAITING
// question/plan of each of the chat's conversations (its digest's
// `waiting`) always loads too, however old (the dock and the inline card).

/** The newest messages a chat loads at first. */
export const CHAT_MESSAGE_LIMIT = CHATS_TRANSCRIPT.window + CHATS_TRANSCRIPT.windowStep
const chatLimits = new Map<string, number>()
const chatLimitListeners = new Set<() => void>()
const subscribeChatLimits = (listener: () => void) => {
  chatLimitListeners.add(listener)
  return () => chatLimitListeners.delete(listener)
}

/** Widen chat `key`'s loaded messages by one `windowStep`. */
export function loadEarlierMessages(key: string): void {
  chatLimits.set(key, (chatLimits.get(key) ?? CHAT_MESSAGE_LIMIT) + CHATS_TRANSCRIPT.windowStep)
  for (const listener of chatLimitListeners) listener()
}

function useChatMessageLimit(key: string | null | undefined): number {
  return useSyncExternalStore(
    subscribeChatLimits,
    () => (key ? chatLimits.get(key) ?? CHAT_MESSAGE_LIMIT : CHAT_MESSAGE_LIMIT),
    () => CHAT_MESSAGE_LIMIT
  )
}

/** A chat's loaded page of messages (`useTranscriptWindow`'s options):
 *  `more` = older ones exist unloaded, `loaded` = how many are loaded,
 *  `loadMore` = one `windowStep` more. */
export interface MessagePage {
  more: boolean
  loaded: number
  ready: boolean
  loadMore: () => void
}

/** The chat's newest messages (by `rootId`, or by `conversationIds`) plus
 *  its conversations' waiting cards; `more` = older ones exist unloaded. */
function useNewestMessages(input: {
  key: string | null | undefined
  rootId?: string | null
  conversationIds?: readonly string[]
  /** The sessions whose digest's waiting card must load. */
  sessionIds: readonly string[]
}): { rows: SyncedMessageRow[]; more: boolean; isReady: boolean } {
  const limit = useChatMessageLimit(input.key)
  const rootId = input.rootId ?? null
  const conversationIds = input.conversationIds ?? []
  const scopeKey = rootId ?? conversationIds.join(`,`)
  const { data, isReady } = useLiveQuery(
    (query) =>
      rootId
        ? query
            .from({ messages: messageCollection })
            .where(({ messages }) => eq(messages.rootId, rootId))
            .orderBy(({ messages }) => messages.createdAt, `desc`)
            .limit(limit)
        : conversationIds.length > 0
          ? query
              .from({ messages: messageCollection })
              .where(({ messages }) => inArray(messages.conversationId, [...conversationIds]))
              .orderBy(({ messages }) => messages.createdAt, `desc`)
              .limit(limit)
          : undefined,
    [scopeKey, limit]
  )
  const sessionKey = input.sessionIds.join(`,`)
  const { data: digests } = useLiveQuery(
    (query) =>
      input.sessionIds.length > 0
        ? query
            .from({ conversations: conversationCollection })
            .where(({ conversations }) => inArray(conversations.sessionId, [...input.sessionIds]))
            .select(({ conversations }) => ({ digest: conversations.digest }))
        : undefined,
    [sessionKey]
  )
  const waitingIds = ((digests ?? []) as { digest: unknown }[])
    .map((row) => (row.digest as ConversationDigest | null)?.waiting?.id)
    .filter((id): id is string => typeof id === `string`)
    .sort()
  const waitingKey = waitingIds.join(`,`)
  const { data: waiting } = useLiveQuery(
    (query) =>
      waitingIds.length > 0
        ? query.from({ messages: messageCollection }).where(({ messages }) => inArray(messages.id, waitingIds))
        : undefined,
    [waitingKey]
  )
  return useMemo(() => {
    const loaded = (data ?? []) as SyncedMessageRow[]
    const ids = new Set(loaded.map((row) => row.id))
    const extra = ((waiting ?? []) as SyncedMessageRow[]).filter((row) => !ids.has(row.id))
    return { rows: [...loaded, ...extra], more: loaded.length >= limit, isReady }
  }, [data, waiting, limit, isReady])
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
  /** #EXP-1380: the loaded newest messages (the transcript window pages
   *  them through `useTranscriptWindow`). */
  messagePage: MessagePage
} {
  const { rows, tree, isReady } = useTreeSessions(rootId)
  const root = rows.find((row) => row.id === rootId) ?? null
  // NEW-1: a row sits on the session that HOLDS the issue; the thread's
  // issues = the union over its tree (`treeIssues`).
  const treeIds = rootId
    ? [...new Set([rootId, ...rows.map((row) => row.id)])].sort()
    : []
  const treeKey = treeIds.join(`,`)
  const { data: links, isReady: linksReady } = useLiveQuery(
    (query) =>
      treeIds.length > 0
        ? query
            .from({ links: sessionIssueCollection })
            .where(({ links }) => inArray(links.sessionId, treeIds))
        : undefined,
    [treeKey]
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
  // EXP-1334: the tree's messages (`root_id`): the root's own rows plus its
  // children's question / plan cards (`buildSessionThread` picks them).
  // #EXP-1380: the NEWEST of them (`useNewestMessages`), Load earlier widens.
  const newest = useNewestMessages({ key: rootId, rootId, sessionIds: treeIds })
  const treeDigests = useDigestMessages({ sessionIds: treeIds })
  const messageRows = newest.rows
  const messageRowsReady = newest.isReady
  // NEW-8: keyed through their conversations; the title waits on both,
  // and on the root's own conversation row (a brand-new thread's lags).
  const { conversations, isReady: conversationsReady } = useConversationSessions()
  const messagesReady =
    messageRowsReady &&
    treeDigests.isReady &&
    conversationsReady &&
    (!rootId || sessionsAwaitingConversation(conversations, [rootId]).size === 0)
  const messages = useMemo(
    () => keyMessageRows((messageRows ?? []) as SyncedMessageRow[], conversations),
    [messageRows, conversations]
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
  // #EXP-1380: titles read the digests (a first prompt older than the
  // loaded messages still names its session).
  const titles = useMemo(
    () =>
      sessionTitles(
        tree,
        ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue),
        foldedIssues,
        treeDigests.messages
      ),
    [tree, links, foldedIssues, treeDigests.messages]
  )
  // #EXP-1380 (chats.json PERF): the cards' clocks tick each second only
  // while a session of the tree works; a settled chat rebuilds per minute.
  const ticking = tree.some((session) => session.status !== `ended` && (session.busy || session.status === `running` || session.status === `queued`))
  const now = useNow(ticking ? 1_000 : 60_000)
  // S-3: the sessions OUTSIDE the tree an xref names (another session's
  // line, the agent conversation that started the root), with titles.
  const linked = useLinkedSessions(root, treeIds, messages)
  const xrefTitles = useMemo(() => {
    if (linked.tree.length === 0) return titles
    const merged = new Map(sessionTitles(linked.tree, [], [], linked.digests))
    for (const [id, title] of titles) merged.set(id, title)
    return merged
  }, [titles, linked])
  const keyVersion = useKeyVersion()
  const view = useMemo(() => {
    if (!root) return null
    return buildSessionThread({
      root: toTreeSession(root),
      sessions: linked.tree.length > 0 ? [...tree, ...linked.tree] : tree,
      sessionIssues: ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue),
      issues: foldedIssues,
      messages: toTreeMessages(messages),
      pullRequests: pullRequests.map(toTreePullRequest),
      now: now.toISOString(),
    }, { focus, titles: xrefTitles })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, tree, linked, links, foldedIssues, messages, pullRequests, now, focus, xrefTitles, keyVersion])
  return {
    view,
    root,
    rows,
    pullRequests,
    isReady,
    titleReady,
    titles,
    messagePage: {
      more: newest.more,
      loaded: newest.rows.length,
      ready: newest.isReady,
      loadMore: () => {
        if (rootId) loadEarlierMessages(rootId)
      },
    },
  }
}

/** S-3: the synced sessions outside a tree its xrefs name: the authors of
 *  its messages from other sessions, their roots (the agent mark), and the
 *  agent conversations of the agent that started the root. An unsynced one
 *  never shows (the contract drops its link). */
export function useLinkedSessions(
  root: SessionRow | null,
  treeIds: readonly string[],
  messages: readonly MessageRow[]
): { tree: TreeSession[]; digests: TreeMessage[] } {
  const inTree = new Set(treeIds)
  // #EXP-1464: and every session a card names (a `thread_started` card, a
  // report, an event line): an agent's threads are roots of their own.
  const cardSessionId = (message: MessageRow): string | null => {
    const id = (message.card as { sessionId?: unknown } | null)?.sessionId
    return typeof id === `string` ? id : null
  }
  const authorIds = [
    ...new Set(
      messages
        .flatMap((message) => [message.authorSessionId, cardSessionId(message)])
        .filter((id): id is string => Boolean(id) && !inTree.has(id as string))
    ),
  ].sort()
  const authorKey = authorIds.join(`,`)
  const { data: authors } = useLiveQuery(
    (query) =>
      authorIds.length > 0
        ? query.from({ sessions: sessionCollection }).where(({ sessions }) => inArray(sessions.id, authorIds))
        : undefined,
    [authorKey]
  )
  const authorRows = (authors ?? []) as SessionRow[]
  const rootIds = [
    ...new Set(
      authorRows
        .map((row) => row.rootId ?? row.id)
        .filter((id) => !inTree.has(id) && !authorIds.includes(id))
    ),
  ].sort()
  const rootKey = rootIds.join(`,`)
  const { data: roots } = useLiveQuery(
    (query) =>
      rootIds.length > 0
        ? query.from({ sessions: sessionCollection }).where(({ sessions }) => inArray(sessions.id, rootIds))
        : undefined,
    [rootKey]
  )
  const starterAgentId = root?.startedByAgentId ?? null
  const { data: starters } = useLiveQuery(
    (query) =>
      starterAgentId
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) => eq(sessions.agentId, starterAgentId))
        : undefined,
    [starterAgentId]
  )
  const rows = useMemo(() => {
    const byId = new Map<string, SessionRow>()
    for (const row of [
      ...authorRows,
      ...((roots ?? []) as SessionRow[]),
      ...((starters ?? []) as SessionRow[]).filter(
        (row) => row.parentId === null && row.userId === root?.userId
      ),
    ]) {
      if (!inTree.has(row.id)) byId.set(row.id, row)
    }
    return [...byId.values()]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authors, roots, starters, root?.userId, treeIds.join(`,`)])
  const ids = useMemo(() => rows.map((row) => row.id).sort(), [rows])
  const { messages: digests } = useDigestMessages({ sessionIds: ids })
  const keyVersion = useKeyVersion()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => ({ tree: rows.map(toTreeSession), digests }), [rows, digests, keyVersion])
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

/** The messages of ONE session (an agent's channel, an opened child): its
 *  own conversation's (NEW-8, keyed through `conversations`), oldest first.
 *  #EXP-1380: its NEWEST ones (`useNewestMessages`; a root shares its
 *  thread's limit), its waiting card always. */
export function useSessionMessages(sessionId: string | null | undefined): MessageRow[] {
  const { conversations } = useConversationSessions()
  const conversationIds = sessionId ? sessionConversationIds(conversations, [sessionId]) : []
  const sessionIds = useMemo(() => (sessionId ? [sessionId] : []), [sessionId])
  const { rows } = useNewestMessages({ key: sessionId, conversationIds, sessionIds })
  // EXP-1459: a new array when a tree key arrives, so readers re-open.
  const keyVersion = useKeyVersion()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => keyMessageRows(rows, conversations), [rows, conversations, keyVersion])
}

/** #EXP-1464 (thread-row-action.json): an expanded session row's waiting
 *  question / plan card (`sessionWaitingCard` over the session's newest
 *  messages + its digest's waiting card, any tree); undefined = loading. */
export function useSessionWaitingCard(
  sessionId: string
): Extract<ThreadRow, { type: `question` | `plan` }> | null | undefined {
  const { conversations } = useConversationSessions()
  const conversationIds = sessionConversationIds(conversations, [sessionId])
  const sessionIds = useMemo(() => [sessionId], [sessionId])
  const { rows, isReady } = useNewestMessages({ key: sessionId, conversationIds, sessionIds })
  const keyVersion = useKeyVersion()
  return useMemo(() => {
    // EXP-1459: sealed cards open here (keyVersion re-opens them once a key lands).
    const messages = toTreeMessages(keyMessageRows(rows, conversations))
    const id = sessionWaitingCard(sessionId, messages)
    const message = id ? messages.find((row) => row.id === id) : undefined
    if (message) return threadAskRow(message)
    return isReady ? null : undefined
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, isReady, conversations, sessionId, keyVersion])
}

/** Every message of ONE tree (`root_id`): an agent's conversation shows
 *  its children's question / plan cards like a thread does. NEW-8: keyed
 *  through `conversations`, oldest first. #EXP-1380: the NEWEST ones, the
 *  tree's waiting cards always; `more` + `loadEarlier` page further back. */
export function useTreeMessages(rootId: string | null | undefined): MessageRow[] & {
  page?: MessagePage
} {
  const { rows: sessions } = useTreeSessions(rootId)
  const sessionIds = useMemo(
    () => (rootId ? [...new Set([rootId, ...sessions.map((row) => row.id)])].sort() : []),
    [rootId, sessions]
  )
  const newest = useNewestMessages({ key: rootId, rootId, sessionIds })
  const origins = useMirrorOrigins(newest.rows)
  const { conversations } = useConversationSessions()
  const keyVersion = useKeyVersion()
  return useMemo(
    () =>
      Object.assign(keyMessageRows([...newest.rows, ...origins], conversations), {
        page: {
          more: newest.more,
          loaded: newest.rows.length,
          ready: newest.isReady,
          loadMore: () => {
            if (rootId) loadEarlierMessages(rootId)
          },
        } satisfies MessagePage,
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [newest.rows, origins, newest.more, newest.isReady, conversations, rootId, keyVersion]
  )
}

/** #EXP-1399 (session-thread.json): the ORIGIN cards of the `ask` mirrors
 *  among `rows` that live in another tree (an escalated card draws the
 *  origin message); a mirror whose origin is already loaded adds nothing.
 *  The shape's visibility rule decides: an origin the viewer cannot see
 *  never syncs, and its mirror draws no card. */
function useMirrorOrigins(rows: readonly SyncedMessageRow[]): SyncedMessageRow[] {
  const missing = useMemo(() => {
    const loaded = new Set(rows.map((row) => row.id))
    const ids = new Set<string>()
    for (const row of rows) {
      const card = row.card as { kind?: unknown; messageId?: unknown } | null
      if (card?.kind === `ask` && typeof card.messageId === `string` && !loaded.has(card.messageId)) {
        ids.add(card.messageId)
      }
    }
    return [...ids].sort()
  }, [rows])
  const key = missing.join(`,`)
  const { data } = useLiveQuery(
    (query) =>
      missing.length > 0
        ? query.from({ messages: messageCollection }).where(({ messages }) => inArray(messages.id, missing))
        : undefined,
    [key]
  )
  return useMemo(() => (missing.length > 0 ? ((data ?? []) as SyncedMessageRow[]) : []), [missing, data])
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
  const key = ids.join(`,`)
  // NEW-1: a session's title reads its OWN rows (root or child alike).
  const { data: links, isReady: linksReady } = useLiveQuery(
    (query) =>
      ids.length > 0
        ? query
            .from({ links: sessionIssueCollection })
            .where(({ links }) => inArray(links.sessionId, ids))
        : undefined,
    [key]
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
  // #EXP-1380: the rows' own conversations' DIGESTS (`useDigestMessages`).
  const { conversations, isReady: conversationsReady } = useConversationSessions()
  const { messages, isReady: digestsReady } = useDigestMessages({ sessionIds: ids })
  const messagesReady = conversationsReady && digestsReady
  // A row whose conversation has not synced yet waits on it instead of
  // reading "Untitled".
  const awaitingKey = [...sessionsAwaitingConversation(conversations, ids)].join(`,`)
  const keyVersion = useKeyVersion()
  return useMemo(() => {
    const sessions = rows.map(toTreeSession)
    const sessionIssues = ((links ?? []) as SessionIssueRow[]).map(toTreeSessionIssue)
    const held = unsettledTitleRoots({
      sessions,
      sessionIssues,
      linksReady: ids.length > 0 && linksReady,
      issuesReady,
      messagesReady: ids.length > 0 && messagesReady,
      awaitingConversation: new Set(awaitingKey ? awaitingKey.split(`,`) : []),
    })
    const titles = sessionTitles(
      sessions.filter((session) => !held.has(session.id)),
      sessionIssues,
      ((issues ?? []) as TreeIssue[]).map(toTreeIssue),
      messages
    )
    return { titles, held, ready: held.size === 0 }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, links, issues, messages, linksReady, issuesReady, messagesReady, awaitingKey, keyVersion])
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

/** The threads (roots) one issue sits in, newest activity first. NEW-1: a
 *  `session_issues` row sits on the session HOLDING the issue (a child as
 *  often as a root), so the holders resolve to their roots. */
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
  const holders = (data ?? []) as SessionRow[]
  const rootIds = [...new Set(holders.map((row) => row.rootId ?? row.id))].sort()
  const rootKey = rootIds.join(`,`)
  const { data: roots } = useLiveQuery(
    (query) =>
      rootIds.length > 0
        ? query
            .from({ sessions: sessionCollection })
            .where(({ sessions }) => inArray(sessions.id, rootIds))
        : undefined,
    [rootKey]
  )
  return useMemo(
    () =>
      ((roots ?? []) as SessionRow[])
        .filter((row) => rootIds.includes(row.id))
        .sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [roots, rootKey]
  )
}
