import { useMemo } from "react"
import {
  agentChatRows,
  appChatRows,
  chatList,
  type AgentChatRow,
  type AppChatRow,
  type ChatList,
  type ChatListView,
} from "@exp/domain-contract/chats"
import { chatWordOf, useTeamPullRequestsByRoot } from "@/components/chat/chats-list"
import { useApps } from "@/hooks/use-apps"
import { useNow } from "@/hooks/use-now"
import {
  useAgents,
  useDigestMessages,
  useSessionReads,
  useTeamSessions,
  useChatsList,
} from "@/hooks/use-session-tree"
import { type ChatRowWord } from "@/lib/session-tree"

// Chat 58cb32df (#EXP-1373, fixture `chats.json`): the Chats
// screen's model (every form factor since #EXP-1380) for the ONE active team — the contract's `chatList` over
// `partitionChats`, filter `all` (#FEED-191: no chips, `chatSections` groups
// it), (S-3) the pinned `agentChatRows` and (#EXP-1733) the `appChatRows`
// between them; `listAgents`
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
  /** #EXP-1388: the Chats list, or the Archived page. */
  view: ChatListView = `list`
): {
  list: ChatList
  /** S-3: the agents pinned on top of the list (none on Archived). */
  agentRows: AgentChatRow[]
  /** #EXP-1733: the team's apps between the agents and the sessions. */
  appRows: AppChatRow[]
  /** A root's agent CLI (its brand mark). */
  cliOf: (sessionId: string) => string | null
  isReady: boolean
} {
  const { groups, isReady } = useChatsList(teamId)
  const { rows, tree } = useTeamSessions(teamId)
  const { agents } = useAgents(teamId, me)
  const { apps } = useApps(teamId)
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
    const words: Record<string, ChatRowWord | null> = {}
    for (const group of groups) {
      for (const row of group.rows) {
        if (row.depth > 0) continue
        const own = prs.byRoot.get(row.sessionId) ?? []
        prStates[row.sessionId] = own.map((pr) => pr.state)
        words[row.sessionId] = chatWordOf(row, own, prs.all)
      }
    }
    return chatList({
      groups,
      messages,
      reads,
      prStates,
      words,
      view,
      now: nowIso,
      utcOffsetMinutes: offset,
      // #EXP-1399 review: an ask mirror of a session I cannot see shows no text.
      visibleSessionIds: tree.map((session) => session.id),
    })
  }, [groups, messages, reads, prs, view, nowIso, offset, tree])

  const agentRows = useMemo(
    () =>
      agentChatRows({
        agents,
        sessions: tree,
        messages,
        reads,
        me: me ?? null,
        view,
        now: nowIso,
        utcOffsetMinutes: offset,
      }),
    [agents, tree, messages, reads, me, view, nowIso, offset]
  )

  const appRows = useMemo(
    () =>
      appChatRows({
        apps: apps.map((app) => ({ appId: app.id, slug: app.slug, name: app.name, icon: app.icon, color: app.color })),
        sessions: tree,
        messages,
        reads,
        me: me ?? null,
        view,
        now: nowIso,
        utcOffsetMinutes: offset,
      }),
    [apps, tree, messages, reads, me, view, nowIso, offset]
  )

  const cliByRoot = useMemo(() => new Map(rows.map((row) => [row.id, row.cli ?? null])), [rows])
  return {
    list,
    agentRows,
    appRows,
    cliOf: (sessionId) => cliByRoot.get(sessionId) ?? null,
    isReady,
  }
}
