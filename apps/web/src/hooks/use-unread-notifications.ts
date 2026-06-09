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
