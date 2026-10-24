import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { boardCollection, issueCollection, notificationCollection } from "@/lib/collections"

// Unread inbox count shared by the sidebar badge and the mobile topbar badge.
// Only call from components rendered when authed — the notifications shape is
// requireAuth, so subscribing it for anonymous viewers would 401.
//
// Thread 58cb32df (chats.json PHONE SHELL): phones show ONE active team, so
// their inbox dot and "Mark all read" count only `teamId`'s notifications —
// a row's own `teamId`, else its issue's board's team (issue notifications
// carry no team, like the server's `markAllRead({teamId})`). No `teamId` =
// every team.
export function useUnreadNotificationCount(teamId?: string | null): number {
  const { data: notifs } = useLiveQuery((query) =>
    query.from({ n: notificationCollection })
  )
  const { data: issues } = useLiveQuery((query) =>
    teamId ? query.from({ i: issueCollection }).select(({ i }) => ({ id: i.id, boardId: i.boardId })) : undefined,
    [teamId]
  )
  const { data: boards } = useLiveQuery((query) =>
    teamId ? query.from({ b: boardCollection }).select(({ b }) => ({ id: b.id, teamId: b.teamId })) : undefined,
    [teamId]
  )
  return useMemo(() => {
    const unread = (notifs ?? []).filter((n) => !n.readAt)
    if (!teamId) return unread.length
    const boardTeam = new Map((boards ?? []).map((b) => [b.id, b.teamId]))
    const issueTeam = new Map((issues ?? []).map((i) => [i.id, boardTeam.get(i.boardId)]))
    return unread.filter((n) => (n.teamId ?? (n.issueId ? issueTeam.get(n.issueId) : null)) === teamId).length
  }, [notifs, issues, boards, teamId])
}
