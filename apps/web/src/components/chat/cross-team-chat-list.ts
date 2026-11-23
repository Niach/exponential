import { useMemo } from "react"
import { and, inArray, isNull, useLiveQuery } from "@tanstack/react-db"
import {
  agentChatRows,
  appChatRows,
  chatList,
  chatMarks,
  sidebarAgents,
  type AgentChatRow,
  type AppChatRow,
  type ChatList,
  type ChatListView,
  type ChatMark,
} from "@exp/domain-contract/chats"
import { sortApps } from "@exp/domain-contract/apps"
import { chatWordOf, useTeamsPullRequestsByRoot } from "@/components/chat/chats-list"
import { useMemberNames, useTeamAgentRows } from "@/hooks/use-agent-row"
import { toListEntry } from "@/hooks/use-apps"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { useNow } from "@/hooks/use-now"
import { localUtcOffsetMinutes } from "@/hooks/use-chat-list"
import {
  useAgents,
  useChatsList,
  useDigestMessages,
  useSessionReads,
  useSessionTitles,
  useTeamSessions,
} from "@/hooks/use-session-tree"
import { appCollection } from "@/lib/collections"
import type { App } from "@/db/schema"
import { sessionWaitingOf, type ChatRowWord, type SessionRow } from "@/lib/session-tree"

// #EXP-2027 (sidebar-tree.json ROOT `chats`): the team-less Chats list of web
// md+, `useChatList` over EVERY member team: the pinned agent rows (the
// viewer's Assistant + every team's agents), the apps, then `chatSections`
// of every team's roots. Each row knows its team (`teamOf`), so it opens and
// is marked in its own team.

export function useCrossTeamChatList(
  teamIds: readonly string[],
  me: string | null | undefined,
  view: ChatListView = `list`
): {
  list: ChatList
  agentRows: AgentChatRow[]
  appRows: AppChatRow[]
  cliOf: (sessionId: string) => string | null
  waitingOf: (sessionId: string) => string | null
  wallOf: (sessionId: string) => { kind: string; device: string | null } | null
  marksOf: (sessionId: string) => readonly ChatMark[]
  /** A root's, an agent's or an app's team id (agents and apps by their id). */
  teamOf: (id: string) => string | null
  /** The session rows of every team (the selection bar reads them). */
  sessionRows: SessionRow[]
  isReady: boolean
} {
  const key = teamIds.join(`,`)
  const { groups, isReady } = useChatsList(teamIds)
  const { rows, tree } = useTeamSessions(teamIds)
  const { titles } = useSessionTitles(rows)
  const waiting = useMemo(() => {
    const out = new Map<string, string | null>()
    for (const row of rows) if (!row.parentId) out.set(row.id, sessionWaitingOf(row, rows, titles))
    return out
  }, [rows, titles])
  const { agents } = useAgents(teamIds, me)
  const { data: appData } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query
            .from({ a: appCollection })
            .where(({ a }) => and(inArray(a.teamId, [...teamIds]), isNull(a.deletedAt)))
        : undefined,
    [key]
  )
  const apps = useMemo(() => sortApps(((appData ?? []) as App[]).map(toListEntry)), [appData])
  const reads = useSessionReads()
  const prs = useTeamsPullRequestsByRoot(teamIds)
  const { messages } = useDigestMessages({ teamIds })
  // The viewer's Assistant converses in their personal team (never a synced
  // team): its pinned row reads that conversation too.
  const teamAgents = useTeamAgentRows(teamIds, me)
  const personal = teamAgents.find((agent) => !teamIds.includes(agent.teamId))?.teamId ?? null
  const agentScope = useMemo(
    () => (personal ? [...teamIds, personal] : [...teamIds]),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [key, personal]
  )
  const { tree: agentTree } = useTeamSessions(agentScope)
  const { messages: agentMessages } = useDigestMessages({ teamIds: agentScope })
  const now = useNow(60_000)
  const nowIso = now.toISOString()
  const offset = localUtcOffsetMinutes(now)

  // #EXP-1990: a teammate's line reads under their name.
  const members = useMemberNames()
  const list = useMemo(() => {
    const prStates: Record<string, string[]> = {}
    const words: Record<string, ChatRowWord | null> = {}
    for (const group of groups) {
      for (const row of group.rows) {
        if (row.depth > 0) continue
        const own = prs.byRoot.get(row.sessionId) ?? []
        prStates[row.sessionId] = own.map((pr) => pr.state)
        words[row.sessionId] = chatWordOf(row, own, prs.all, waiting.get(row.sessionId) ?? null)
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
      visibleSessionIds: tree.map((session) => session.id),
      me: me ?? null,
      members,
    })
  }, [groups, messages, reads, prs, view, nowIso, offset, tree, waiting, me, members])

  const agentRows = useMemo(
    () =>
      agentChatRows({
        agents: sidebarAgents(agents, apps.map((app) => ({ agentId: app.row?.agentId ?? null }))),
        sessions: agentTree,
        messages: agentMessages,
        reads,
        me: me ?? null,
        members,
        view,
        now: nowIso,
        utcOffsetMinutes: offset,
      }),
    [agents, apps, agentTree, agentMessages, reads, me, members, view, nowIso, offset]
  )

  const appRows = useMemo(
    () =>
      appChatRows({
        apps: apps.map((app) => ({ appId: app.id, slug: app.slug, name: app.name, icon: app.icon, color: app.color })),
        sessions: tree,
        messages,
        reads,
        me: me ?? null,
        members,
        view,
        now: nowIso,
        utcOffsetMinutes: offset,
      }),
    [apps, tree, messages, reads, me, members, view, nowIso, offset]
  )

  const deviceGlyph = useDeviceGlyphs()
  const walls = useMemo(() => {
    const out = new Map<string, { kind: string; deviceId: string | null }>()
    for (const group of groups) for (const row of group.rows) if (row.wall) out.set(row.sessionId, row.wall)
    return out
  }, [groups])
  const cliByRoot = useMemo(() => new Map(rows.map((row) => [row.id, row.cli ?? null])), [rows])
  const marksByRoot = useMemo(
    () => new Map(rows.filter((row) => !row.parentId).map((row) => [row.id, chatMarks(row)])),
    [rows]
  )
  const teams = useMemo(() => {
    const out = new Map<string, string>()
    for (const row of rows) out.set(row.id, row.teamId)
    for (const agent of teamAgents) out.set(agent.id, agent.teamId)
    for (const app of apps) if (app.row) out.set(app.id, app.row.teamId)
    return out
  }, [rows, teamAgents, apps])

  return {
    list,
    agentRows,
    appRows,
    cliOf: (sessionId) => cliByRoot.get(sessionId) ?? null,
    waitingOf: (sessionId) => waiting.get(sessionId) ?? null,
    wallOf: (sessionId) => {
      const wall = walls.get(sessionId)
      return wall ? { kind: wall.kind, device: deviceGlyph(wall.deviceId)?.name ?? null } : null
    },
    marksOf: (sessionId) => marksByRoot.get(sessionId) ?? [],
    teamOf: (id) => teams.get(id) ?? null,
    sessionRows: rows,
    isReady,
  }
}
