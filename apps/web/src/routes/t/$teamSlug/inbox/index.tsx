import { useMemo } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { InboxView } from "@/components/inbox/inbox-view"
import { MyIssuesView } from "@/components/my-issues-view"
import { Button } from "@/components/ui/button"
import { useSession } from "@/hooks/use-session"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { trpc } from "@/lib/trpc-client"
import type { IssueFilterSearch, IssueFilters } from "@/lib/filters"
import {
  issueFilterSearchFromFilters,
  issueFiltersFromSearch,
  parseIssueFilterSearch,
} from "@/lib/filters"
import { cn } from "@/lib/utils"

// The merged personal surface (EXP-186): ONE sidebar entry ("Inbox") with two
// tabs — the notification stream and the cross-board My Issues list — matching
// the mobile apps' segmented My Work screen. The active tab lives in the URL
// (?tab=my-issues; absent = inbox) alongside the My Issues filter params so
// both tabs stay shareable and survive refresh.
type InboxSearch = IssueFilterSearch & {
  tab?: `my-issues`
}

export const Route = createFileRoute(`/t/$teamSlug/inbox/`)({
  // Filter parse/serialize is shared with the board routes (lib/filters.ts) so
  // the two surfaces can't drift — EXP-314 widened `status` to accept
  // issue_statuses row uuids alongside the legacy anchor-enum tokens.
  validateSearch: (search: Record<string, unknown>): InboxSearch => ({
    tab: search.tab === `my-issues` ? `my-issues` : undefined,
    ...parseIssueFilterSearch(search),
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: InboxPage,
})

function UnreadCountPill({ active }: { active: boolean }) {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return (
    <span
      className={cn(
        `rounded-full px-1.5 py-px text-[10px] font-medium tabular-nums`,
        active
          ? `bg-brand-strong text-brand-foreground`
          : `bg-muted text-muted-foreground`
      )}
    >
      {unread > 99 ? `99+` : unread}
    </span>
  )
}

function MarkAllReadButton() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={() => void trpc.notifications.markAllRead.mutate()}
    >
      Mark all read
    </Button>
  )
}

function InboxPage() {
  const { teamSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const { data: session } = useSession()
  const tab = search.tab === `my-issues` ? `my-issues` : `inbox`

  const filters = useMemo<IssueFilters>(
    () => issueFiltersFromSearch(search),
    [search.status, search.priority, search.labels]
  )

  const setFilters = (next: IssueFilters) => {
    void navigate({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug },
      search: {
        tab: `my-issues`,
        ...issueFilterSearchFromFilters(next),
      },
      replace: true,
    })
  }

  const setTab = (next: `inbox` | `my-issues`) => {
    void navigate({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug },
      search: {
        ...search,
        tab: next === `my-issues` ? `my-issues` : undefined,
      },
      replace: true,
    })
  }

  if (!session?.user) return null

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center justify-between px-4 pt-3 md:px-6">
        <div className="flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setTab(`inbox`)}
            className={cn(
              `shrink-0 gap-1.5 rounded-full h-7 px-3 text-xs`,
              tab === `inbox`
                ? `bg-accent text-foreground font-medium`
                : `text-muted-foreground hover:text-foreground`
            )}
          >
            Inbox
            <UnreadCountPill active={tab === `inbox`} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setTab(`my-issues`)}
            className={cn(
              `shrink-0 rounded-full h-7 px-3 text-xs`,
              tab === `my-issues`
                ? `bg-accent text-foreground font-medium`
                : `text-muted-foreground hover:text-foreground`
            )}
          >
            My Issues
          </Button>
        </div>
        {tab === `inbox` && <MarkAllReadButton />}
      </div>

      <div className="min-h-0 flex-1">
        {tab === `inbox` ? (
          <InboxView teamSlug={teamSlug} />
        ) : (
          <MyIssuesView
            teamSlug={teamSlug}
            filters={filters}
            onFiltersChange={setFilters}
          />
        )}
      </div>
    </div>
  )
}
