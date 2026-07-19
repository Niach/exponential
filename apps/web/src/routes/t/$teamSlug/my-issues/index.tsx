import { useMemo } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { CircleUser } from "lucide-react"
import { EmptyState } from "@/components/empty-state"
import { IssueFilterBar } from "@/components/issue-filter-bar"
import { IssueList } from "@/components/issue-list"
import { useMyIssuesData } from "@/hooks/use-my-issues-data"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { hasActiveFilters as filtersActive } from "@/lib/filters"
import type { IssueFilters } from "@/lib/filters"
import { issuePriorityOptions, issueStatusOptions } from "@/lib/domain"
import type { IssuePriority, IssueStatus } from "@/lib/domain"

// Cross-board "My Issues": every issue assigned to the signed-in user across
// all boards in the team, grouped by status like the board
// (masterplan §5a — a fixed built-in view, no saved-filter machinery). Rows
// span boards, so the identifier column (always `{PREFIX}-{number}`) carries
// the board context; clicking a row opens the full-page detail route.
//
// Filters live in the URL like the board (?status=…&priority=…&labels=…)
// so a filtered view is shareable and survives refresh.
type MyIssuesSearch = {
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

export const Route = createFileRoute(`/t/$teamSlug/my-issues/`)({
  validateSearch: (search: Record<string, unknown>): MyIssuesSearch => ({
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
  component: MyIssuesPage,
})

function MyIssuesPage() {
  const { teamSlug } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const { data: session } = useSession()
  const userId = session?.user?.id

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
      to: `/t/$teamSlug/my-issues`,
      params: { teamSlug },
      search: {
        status: next.statuses.length ? next.statuses.join(`,`) : undefined,
        priority: next.priorities.length
          ? next.priorities.join(`,`)
          : undefined,
        labels: next.labelIds.length ? next.labelIds.join(`,`) : undefined,
      },
      replace: true,
    })
  }

  const {
    issueLabelMap,
    issuesReady,
    labelList,
    boardMap,
    totalIssueCount,
    users,
    userMap,
    visibleGroups,
    team,
  } = useMyIssuesData({ filters, userId, teamSlug })

  const permissions = useTeamPermissions(team)

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  return (
    <div className="flex flex-col h-full">
      <IssueFilterBar
        title="My Issues"
        filters={filters}
        onFiltersChange={setFilters}
        labels={labelList}
        onNewIssue={() => {}}
        canCreate={false}
      />

      <div className="flex-1 overflow-auto">
        {issuesReady && totalIssueCount === 0 ? (
          <EmptyState
            icon={CircleUser}
            title="No issues assigned to you"
            description="Issues assigned to you across all boards in this team will show up here."
          />
        ) : (
          <IssueList
            groups={visibleGroups}
            issueLabelMap={issueLabelMap}
            labels={labelList}
            users={users}
            userMap={userMap}
            onNewIssue={() => {}}
            onIssueClick={(issue) => {
              const board = boardMap.get(issue.boardId)
              if (!board) return
              void navigate({
                to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
                params: {
                  teamSlug,
                  boardSlug: board.slug,
                  issueIdentifier: issue.identifier,
                },
              })
            }}
            canCreate={false}
            canMutateIssue={permissions.canMutateIssue}
            canModerate={permissions.isModerator}
            bulkTeamId={team.id}
            isLoading={!issuesReady}
            hasAnyIssues={totalIssueCount > 0}
            hasActiveFilters={filtersActive(filters)}
            onClearFilters={() =>
              setFilters({ statuses: [], priorities: [], labelIds: [] })
            }
          />
        )}
      </div>
    </div>
  )
}
