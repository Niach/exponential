import { useMemo } from "react"
import {
  agentBubbles,
  agentChatRows,
  chatList,
  type AgentBubble,
  type AgentChatRow,
  type ChatFilter,
  type ChatList,
  type ChatListView,
} from "@exp/domain-contract/chats"
import { threadWordOf, useTeamPullRequestsByRoot } from "@/components/thread/threads-list"
import { useNow } from "@/hooks/use-now"
import {
  useAgents,
  useDigestMessages,
  useSessionReads,
  useTeamSessions,
  useThreadsList,
} from "@/hooks/use-session-tree"
import { type ThreadRowWord } from "@/lib/session-tree"

// Thread 58cb32df (#EXP-1373, fixture `chats.json`): the Chats
// screen's model (every form factor since #EXP-1380) for the ONE active team — the contract's `chatList` over
// `partitionThreads` (the same groups the wide Chats page bands), and the
// agents as `agentBubbles` and (S-3) the pinned `agentChatRows`; `listAgents`
// reads each agent's `role`/`ownerId` (the agents shape syncs both), so
// another member's Assistant never shows. Pure reads over synced shapes; nothing here
// decides a rule.

/** The viewer's local UTC offset in minutes (`chatTimeLabel`'s day). */
export function localUtcOffsetMinutes(now: Date): number {
  return -now.getTimezoneOffset()
}

export function useChatList(
  teamId: string | undefined,
  me: string | null | undefined,
  filter: ChatFilter,
  /** #EXP-1388: the Chats list, or the Archived page. */
  view: ChatListView = `list`
): {
  list: ChatList
  bubbles: AgentBubble[]
  /** S-3: the agents pinned on top of the list (none on Archived). */
  agentRows: AgentChatRow[]
  /** A root's agent CLI (its brand mark). */
  cliOf: (sessionId: string) => string | null
  isReady: boolean
} {
  const { groups, isReady } = useThreadsList(teamId)
  const { rows, tree } = useTeamSessions(teamId)
  const { agents } = useAgents(teamId, me)
  const reads = useSessionReads()
  const prs = useTeamPullRequestsByRoot(teamId)
  // #EXP-1380: the digests, never the transcripts (`useDigestMessages`).
  const teamIds = useMemo(() => (teamId ? [teamId] : []), [teamId])
  const { messages } = useDigestMessages({ teamIds })
  const now = useNow(60_000)
  const nowIso = now.toISOString()
  const offset = localUtcOffsetMinutes(now)

  const list = useMemo(() => {
    const prStates: Record<string, string[]> = {}
    const words: Record<string, ThreadRowWord | null> = {}
    for (const group of groups) {
      for (const row of group.rows) {
        if (row.depth > 0) continue
        const own = prs.byRoot.get(row.sessionId) ?? []
        prStates[row.sessionId] = own.map((pr) => pr.state)
        words[row.sessionId] = threadWordOf(row, own, prs.all)
      }
    }
    return chatList({
      groups,
      messages,
      reads,
      prStates,
      words,
      filter,
      view,
      now: nowIso,
      utcOffsetMinutes: offset,
      // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
      visibleSessionIds: tree.map((session) => session.id),
    })
  }, [groups, messages, reads, prs, filter, view, nowIso, offset, tree])

  const bubbles = useMemo(
    () => agentBubbles({ agents, sessions: tree, messages, reads, me: me ?? null }),
    [agents, tree, messages, reads, me]
  )

  const agentRows = useMemo(
    () =>
      agentChatRows({
        agents,
        sessions: tree,
        messages,
        reads,
        me: me ?? null,
        filter,
        view,
        now: nowIso,
        utcOffsetMinutes: offset,
      }),
    [agents, tree, messages, reads, me, filter, view, nowIso, offset]
  )

  const cliByRoot = useMemo(() => new Map(rows.map((row) => [row.id, row.cli ?? null])), [rows])
  return {
    list,
    bubbles,
    agentRows,
    cliOf: (sessionId) => cliByRoot.get(sessionId) ?? null,
    isReady,
  }
}
