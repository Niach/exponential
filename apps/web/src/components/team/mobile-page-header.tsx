import type { ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { PageHeader } from "@exp/ui"
import { useIssueSearch } from "@/hooks/use-issue-search"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"

// #EXP-1384 (chats.json `shell.header`): the web-mobile wiring of THE phone
// page header on every top-level page: search opens the team's issue search
// sheet, the inbox circle pushes the inbox page and wears (#FEED-232) the
// count badge of the ACTIVE team's unread notifications. `md:hidden` — wide pages keep
// their own headers.
export function MobilePageHeader({
  teamSlug,
  title,
  leading,
  className,
}: {
  teamSlug: string
  title: ReactNode
  leading?: ReactNode
  className?: string
}) {
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const unread = useUnreadNotificationCount(team?.id)
  const issueSearch = useIssueSearch()
  return (
    <PageHeader
      title={title}
      leading={leading}
      inboxCount={unread}
      onSearch={issueSearch.open}
      onInbox={() => void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug } })}
      className={className}
    />
  )
}
