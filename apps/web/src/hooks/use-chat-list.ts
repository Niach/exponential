import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  agentBubbles,
  chatList,
  type AgentBubble,
  type ChatFilter,
  type ChatList,
} from "@exp/domain-contract/chats"
import { threadWordOf, useTeamPullRequestsByRoot } from "@/components/thread/threads-list"
import { useNow } from "@/hooks/use-now"
import {
  useAgents,
  useConversationSessions,
  useSessionReads,
  useTeamSessions,
  useThreadsList,
} from "@/hooks/use-session-tree"
import { messageCollection } from "@/lib/collections"
import {
  keyMessageRows,
  toTreeMessage,
  type SyncedMessageRow,
  type ThreadRowWord,
} from "@/lib/session-tree"

// Thread 58cb32df (#EXP-1373, fixture `chats.json`): the PHONE Chats
// screen's model for the ONE active team — the contract's `chatList` over
// `partitionThreads` (the same groups the wide Chats page bands), and the
// agents as `agentBubbles`. Pure reads over synced shapes; nothing here
// decides a rule.

/** The viewer's local UTC offset in minutes (`chatTimeLabel`'s day). */
export function localUtcOffsetMinutes(now: Date): number {
  return -now.getTimezoneOffset()
}

export function useChatList(
  teamId: string | undefined,
  me: string | null | undefined,
  filter: ChatFilter
): {
  list: ChatList
  bubbles: AgentBubble[]
  /** A root's agent CLI (its brand mark). */
  cliOf: (sessionId: string) => string | null
  isReady: boolean
} {
  const { groups, isReady } = useThreadsList(teamId)
  const { rows, tree } = useTeamSessions(teamId)
  const { agents } = useAgents(teamId, me)
  const reads = useSessionReads()
  const prs = useTeamPullRequestsByRoot(teamId)
  const { data: messageRows } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ messages: messageCollection }).where(({ messages }) => eq(messages.teamId, teamId))
        : undefined,
    [teamId]
  )
  const { conversations } = useConversationSessions()
  const messages = useMemo(
    () => keyMessageRows((messageRows ?? []) as SyncedMessageRow[], conversations).map(toTreeMessage),
    [messageRows, conversations]
  )
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
      now: nowIso,
      utcOffsetMinutes: offset,
    })
  }, [groups, messages, reads, prs, filter, nowIso, offset])

  const bubbles = useMemo(
    () => agentBubbles({ agents, sessions: tree, messages, reads, me: me ?? null }),
    [agents, tree, messages, reads, me]
  )

  const cliByRoot = useMemo(() => new Map(rows.map((row) => [row.id, row.cli ?? null])), [rows])
  return {
    list,
    bubbles,
    cliOf: (sessionId) => cliByRoot.get(sessionId) ?? null,
    isReady,
  }
}
