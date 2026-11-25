import type { ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { PageHeader } from "@exp/ui"
import { useIssueSearch } from "@/hooks/use-issue-search"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"

// #EXP-2109 (chats.json `shell.header`): the web-mobile wiring of THE phone
// page header on the root pages (Chats, Teams, Settings): search opens the
// every-team search sheet, the inbox circle pushes the inbox page and wears
// the count badge of every team's unread notifications.
export function MobilePageHeader({
  title,
  leading,
  trailing,
  className,
}: {
  title: ReactNode
  leading?: ReactNode
  /** The page's own circles, after search and inbox. */
  trailing?: ReactNode
  className?: string
}) {
  const navigate = useNavigate()
  const unread = useUnreadNotificationCount()
  const issueSearch = useIssueSearch()
  return (
    <PageHeader
      title={title}
      leading={leading}
      trailing={trailing}
      inboxCount={unread}
      onSearch={issueSearch.open}
      onInbox={() => void navigate({ to: `/inbox` })}
      className={className}
    />
  )
}

/** A pushed phone page's header title: the leading glyph (a team's mark, a
 *  folder, a board's glyph) and the name, beside the back control. */
export function PushedTitle({ glyph, children }: { glyph?: ReactNode; children: ReactNode }) {
  return (
    <span className="flex min-w-0 items-center gap-2.5 text-left">
      {glyph && <span className="flex size-6 shrink-0 items-center justify-center">{glyph}</span>}
      <span className="min-w-0 truncate text-[17px] font-semibold">{children}</span>
    </span>
  )
}
