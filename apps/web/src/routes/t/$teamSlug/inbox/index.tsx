import { useMemo, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { InboxView } from "@/components/inbox/inbox-view"
import {
  MyIssuesFilterAction,
  MyIssuesView,
} from "@/components/my-issues-view"
import { Button } from "@/components/ui/button"
import {
  SEGMENTED_ROW,
  SEGMENTED_TAB,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import { conceptIcon } from "@/lib/icons.generated"
import { useSession } from "@/hooks/use-session"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { trpc } from "@/lib/trpc-client"
import type { IssueFilterSearch, IssueFilters } from "@/lib/filters"
import {
  issueFilterSearchFromFilters,
  issueFiltersFromSearch,
  parseIssueFilterSearch,
} from "@/lib/filters"

// EXP-525: the tab segments carry the same registry glyphs the mobile My Work
// segments and the desktop rail use.
const InboxTabIcon = conceptIcon(`nav-inbox`)
const MyIssuesTabIcon = conceptIcon(`ui-assignee`)

// The merged personal surface (EXP-186): ONE sidebar entry ("Inbox") with two
// tabs — the notification stream and the cross-board My Issues list — matching
// the mobile apps' segmented My Work screen. The active tab lives in the URL
// (?tab=my-issues; absent = inbox) alongside the My Issues filter params so
// both tabs stay shareable and survive refresh.
//
// EXP-851: a LIST, nothing else. The md+ split pane (and its `?issue=`
// selection) is gone — a row opens the issue's own route carrying
// `?from=inbox` / `?from=inbox:my-issues`, and the sidebar shows this list
// beside it instead of a second column inside the page.
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

// EXP-616: inside the capsule segment the count is a plain trailing number
// (the GlassSectionHeader idiom), not a second pill nested in a pill.
function UnreadTabCount() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return (
    <span className="text-xs text-foreground/50 tabular-nums">
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
  const [bulkSlot, setBulkSlot] = useState<HTMLDivElement | null>(null)

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
      {/* EXP-851: `SEGMENTED_ROW` — the same padding the Support strip sits
          in, so the two read as one control at one size. */}
      <div className={SEGMENTED_ROW}>
        <div className="flex min-w-0 items-center gap-2">
          {/* EXP-616: the capsule segmented control, still URL-driven — the
              controlled value is the parsed ?tab and every change navigates. */}
          <Tabs
            value={tab}
            onValueChange={(next) => setTab(next as `inbox` | `my-issues`)}
            className="w-fit shrink-0"
          >
            <TabsList>
              <TabsTrigger value="inbox" className={SEGMENTED_TAB}>
                <InboxTabIcon />
                Inbox
                <UnreadTabCount />
              </TabsTrigger>
              <TabsTrigger value="my-issues" className={SEGMENTED_TAB}>
                <MyIssuesTabIcon />
                My Issues
              </TabsTrigger>
            </TabsList>
          </Tabs>
          {/* The My Issues bulk-action bar portals in here (EXP-525) so a
              selection never reflows the list under it; EXP-642 moved the
              slot LEFT, beside the tabs, away from the filter trigger. */}
          {tab === `my-issues` && (
            <div ref={setBulkSlot} className="contents" />
          )}
        </div>
        {tab === `inbox` ? (
          <MarkAllReadButton />
        ) : (
          <MyIssuesFilterAction
            teamSlug={teamSlug}
            filters={filters}
            onFiltersChange={setFilters}
          />
        )}
      </div>

      <div className="min-h-0 flex-1">
        {tab === `inbox` ? (
          <InboxView teamSlug={teamSlug} from="inbox" />
        ) : (
          <MyIssuesView
            teamSlug={teamSlug}
            filters={filters}
            onFiltersChange={setFilters}
            bulkActionSlot={bulkSlot}
          />
        )}
      </div>
    </div>
  )
}
