import { useMemo } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { InboxView } from "@/components/inbox/inbox-view"
import { MyIssuesView } from "@/components/my-issues-view"
import { Button } from "@/components/ui/button"
import { useSession } from "@/hooks/use-session"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { trpc } from "@/lib/trpc-client"
import type { IssueFilters } from "@/lib/filters"
import { issuePriorityOptions, issueStatusOptions } from "@/lib/domain"
import type { IssuePriority, IssueStatus } from "@/lib/domain"
import { cn } from "@/lib/utils"

// The merged personal surface (EXP-186): ONE sidebar entry ("Inbox") with two
// tabs — the notification stream and the cross-board My Issues list — matching
// the mobile apps' segmented My Work screen. The active tab lives in the URL
// (?tab=my-issues; absent = inbox) alongside the My Issues filter params so
// both tabs stay shareable and survive refresh.
type InboxSearch = {
  tab?: `my-issues`
  status?: string
  priority?: string
  labels?: string
}

const STATUS_VALUES = issueStatusOptions.map((o) => o.value)
const PRIORITY_VALUES = issuePriorityOptions.map((o) => o.value)

// Coerce a raw search value (array or comma string) to a validated,
// comma-joined string, or undefined when empty (mirrors the board index).
function validatedCsv(
  raw: unknown,
  allowed?: readonly string[]
): string | undefined {
  let arr: string[]
  if (Array.isArray(raw)) {
    arr = raw.filter((v): v is string => typeof v === `string`)
  } else if (typeof raw === `string` && raw.length > 0) {
    arr = raw.split(`,`)
  } else {
    return undefined
  }
  const cleaned = allowed
    ? arr.filter((v) => allowed.includes(v))
    : arr.filter((v) => v.length > 0)
  return cleaned.length ? cleaned.join(`,`) : undefined
}

export const Route = createFileRoute(`/t/$teamSlug/inbox/`)({
  validateSearch: (search: Record<string, unknown>): InboxSearch => ({
    tab: search.tab === `my-issues` ? `my-issues` : undefined,
    status: validatedCsv(search.status, STATUS_VALUES),
    priority: validatedCsv(search.priority, PRIORITY_VALUES),
    labels: validatedCsv(search.labels),
  }),
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
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
          ? `bg-indigo-600 text-white`
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
    () => ({
      statuses: search.status
        ? (search.status.split(`,`) as IssueStatus[])
        : [],
      priorities: search.priority
        ? (search.priority.split(`,`) as IssuePriority[])
        : [],
      labelIds: search.labels ? search.labels.split(`,`) : [],
    }),
    [search.status, search.priority, search.labels]
  )

  const setFilters = (next: IssueFilters) => {
    void navigate({
      to: `/t/$teamSlug/inbox`,
      params: { teamSlug },
      search: {
        tab: `my-issues`,
        status: next.statuses.length ? next.statuses.join(`,`) : undefined,
        priority: next.priorities.length
          ? next.priorities.join(`,`)
          : undefined,
        labels: next.labelIds.length ? next.labelIds.join(`,`) : undefined,
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
