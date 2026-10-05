import { EmptyState, ListRow, LiveDot, cn, conceptIcon } from "@exp/ui"
import type { NotificationRow } from "./model"
import { inboxGroups, notificationConcept, type InboxGroup } from "./list-inbox"
import { relativeTime } from "./time"

const AllCaughtUpIcon = conceptIcon(`status-done`)

// EXP-1183 — `exponential_notifications_list` as the app's inbox page
// (`components/inbox/inbox-view.tsx`): flat two-line reading rows — the
// type's glyph in its muted circle, the title (bold while unread), the time
// and the unread dot, the body underneath; read rows dim. An issue's
// notifications fold into one row (its newest leads, `+N` counts the rest).
// A row with an issue opens it (the host calls `exponential_issues_get`).
export function InboxView({
  notifications,
  onOpen,
}: {
  notifications: readonly NotificationRow[]
  onOpen?: (notification: NotificationRow) => void
}) {
  const groups = inboxGroups(notifications)
  if (groups.length === 0) {
    return (
      <EmptyState
        icon={AllCaughtUpIcon}
        title="All caught up"
        description="Assignments, comments and mentions on issues you follow will show up here."
      />
    )
  }
  return (
    <div className="flex flex-col p-2">
      {groups.map((group) => (
        <InboxRow
          key={group.key}
          group={group}
          onOpen={group.items[0]!.issueId ? onOpen : undefined}
        />
      ))}
    </div>
  )
}

/** The inbox's own stamp: `just now`, `5m`, `3h`, `2d`. */
function inboxTime(value: string): string {
  const short = relativeTime(value)
  return short === `now` ? `just now` : short
}

function InboxRow({
  group,
  onOpen,
}: {
  group: InboxGroup
  onOpen?: (notification: NotificationRow) => void
}) {
  const latest = group.items[0]!
  const unread = group.unread > 0
  const Icon = conceptIcon(notificationConcept(latest.type))
  const more = group.items.length - 1
  return (
    <ListRow
      interactive={Boolean(onOpen)}
      onClick={onOpen ? () => onOpen(latest) : undefined}
      className={cn(`items-start px-3 py-2`, !unread && `opacity-60`)}
      data-testid={`inbox-row-${latest.id}`}
    >
      <div className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted">
        <Icon className="size-3.5 text-muted-foreground" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className={cn(`truncate text-sm`, unread && `font-medium`)}>{latest.title}</span>
          {more > 0 && (
            <span className="shrink-0 text-xs tabular-nums text-muted-foreground">+{more}</span>
          )}
          <span className="ml-auto w-16 shrink-0 text-right text-xs text-muted-foreground">
            {inboxTime(latest.createdAt)}
          </span>
          <span className="w-2 shrink-0" aria-hidden>
            {unread && <LiveDot tone="unread" className="block" />}
          </span>
        </div>
        {latest.body?.trim() && (
          <div className="mt-0.5 truncate text-xs text-muted-foreground">{latest.body}</div>
        )}
      </div>
    </ListRow>
  )
}
