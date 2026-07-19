import { useLiveQuery } from "@tanstack/react-db"
import { notificationCollection } from "@/lib/collections"

// Unread inbox count shared by the sidebar badge and the mobile topbar badge.
// Only call from components rendered when authed — the notifications shape is
// requireAuth, so subscribing it for anonymous viewers would 401.
export function useUnreadNotificationCount(): number {
  const { data: notifs } = useLiveQuery((query) =>
    query.from({ n: notificationCollection })
  )
  return (notifs ?? []).filter((n) => !n.readAt).length
}

// Unread helpdesk count for one team's Support entry: issue-less
// support_reply rows carry a synced team_id (the inbox's per-team Support
// groups use the same rule). Same auth caveat as above.
export function useUnreadSupportCount(teamId: string | undefined): number {
  const { data: notifs } = useLiveQuery((query) =>
    query.from({ n: notificationCollection })
  )
  if (!teamId) return 0
  return (notifs ?? []).filter(
    (n) =>
      n.type === `support_reply` &&
      !n.issueId &&
      n.teamId === teamId &&
      !n.readAt
  ).length
}
