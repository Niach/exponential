import { EmptyState, LiveDot, conceptIcon } from "@exp/ui"
import type { NotificationRow } from "./model"
import { relativeTime } from "./time"

const InboxIcon = conceptIcon(`nav-inbox`)

// EXP-1183 — `exponential_notifications_list` as the inbox: flat hairline
// rows, the unread dot leading (the inbox's `unread` tone), title, the body's
// first line and when. A row with an issue opens it (the host calls
// `exponential_issues_get`).
export function InboxView({
  notifications,
  onOpen,
}: {
  notifications: readonly NotificationRow[]
  onOpen?: (notification: NotificationRow) => void
}) {
  if (notifications.length === 0) {
    return (
      <EmptyState
        icon={InboxIcon}
        title="Inbox zero"
        description="No notifications here."
      />
    )
  }
  return (
    <div className="flex flex-col">
      {notifications.map((notification) => (
        <InboxRow
          key={notification.id}
          notification={notification}
          onOpen={notification.issueId ? onOpen : undefined}
        />
      ))}
    </div>
  )
}

function InboxRow({
  notification,
  onOpen,
}: {
  notification: NotificationRow
  onOpen?: (notification: NotificationRow) => void
}) {
  const unread = notification.readAt === null
  return (
    <div
      role={onOpen ? `button` : undefined}
      tabIndex={onOpen ? 0 : undefined}
      onClick={() => onOpen?.(notification)}
      onKeyDown={(event) => {
        if (onOpen && (event.key === `Enter` || event.key === ` `)) {
          event.preventDefault()
          onOpen(notification)
        }
      }}
      className={`flex items-start gap-3 border-b border-border/30 px-4 py-3 outline-none ${onOpen ? `cursor-pointer hover:bg-glass-row focus-visible:bg-glass-row` : ``}`}
    >
      <span className="flex h-5 w-2 shrink-0 items-center">
        {unread && <LiveDot tone="unread" label="Unread" className="size-2" />}
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex items-baseline gap-2">
          <span className={`truncate text-sm ${unread ? `font-medium` : `text-muted-foreground`}`}>
            {notification.title}
          </span>
          <span className="ml-auto shrink-0 text-xs text-muted-foreground">
            {relativeTime(notification.createdAt)}
          </span>
        </div>
        {notification.body?.trim() && (
          <span className="line-clamp-2 text-xs text-muted-foreground">
            {notification.body}
          </span>
        )}
      </div>
    </div>
  )
}
