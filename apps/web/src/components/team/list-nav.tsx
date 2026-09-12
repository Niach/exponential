import { useMemo, useState } from "react"
import { Link, useNavigate, useParams } from "@tanstack/react-router"
import { GitPullRequest } from "lucide-react"
import type { Board, Team } from "@/db/schema"
import {
  originLabel,
  type DetailOrigin,
} from "@/lib/detail-origin"
import { emptyFilters } from "@/lib/filters"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { useMyIssuesData } from "@/hooks/use-my-issues-data"
import { useReviewsData } from "@/hooks/use-reviews-data"
import { useSession } from "@/hooks/use-session"
import { conceptIcon } from "@/lib/icons.generated"
import { BoardIssueListPane } from "@/components/board-issue-list-pane"
import { InboxView } from "@/components/inbox/inbox-view"
import { SupportThreadList } from "@/components/helpdesk/support-inbox"
import { SessionsList } from "@/components/agent-shell"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
import {
  SEGMENTED_ROW_COMPACT,
  SEGMENTED_TAB,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"

// EXP-851: the sidebar's THIRD panel — the list a detail came from, in the
// 16rem slot the main menu and the settings nav share. It slides in exactly
// like settings does, carries the same back row (`SidebarBackRow`, labelled
// with the list) and the list itself, simplified: a board's issue rows, the
// inbox stream, the support threads, the Agent page's runs, the review queue.
// Which panel is up is a pure function of the URL (`sidebarOccupant`), so a
// deep link lands settled and the sidebar can never disagree with the page.
//
// Every row navigates to ITS detail carrying the same `?from=` token, so the
// nav survives walking a list one detail at a time.

const InboxTabIcon = conceptIcon(`nav-inbox`)
const MyIssuesTabIcon = conceptIcon(`ui-assignee`)

export function TeamListNav({
  teamSlug,
  team,
  boards,
  origin,
}: {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
  origin: DetailOrigin
}) {
  const navigate = useNavigate()
  const boardSlug =
    origin.kind === `board` || origin.kind === `issue` ? origin.boardSlug : null
  const board = boardSlug
    ? (boards?.find((row) => row.slug === boardSlug) ?? null)
    : null

  const goBack = () => {
    switch (origin.kind) {
      case `board`:
      case `issue`:
        void navigate({
          to: `/t/$teamSlug/boards/$boardSlug`,
          params: { teamSlug, boardSlug: origin.boardSlug },
          search: {},
        })
        return
      case `inbox`:
        void navigate({
          to: `/t/$teamSlug/inbox`,
          params: { teamSlug },
          search: origin.tab === `my-issues` ? { tab: `my-issues` } : {},
        })
        return
      case `support`:
        void navigate({ to: `/t/$teamSlug/support`, params: { teamSlug } })
        return
      case `reviews`:
        void navigate({ to: `/t/$teamSlug/reviews`, params: { teamSlug } })
        return
      case `agent`:
        void navigate({ to: `/t/$teamSlug/agent`, params: { teamSlug } })
    }
  }

  return (
    <>
      <SidebarBackRow label={originLabel(origin, board?.name)} onBack={goBack} />
      {/* A plain column, not `SidebarContent`: each list below owns its own
          scrollport, and two nested `overflow-auto` boxes make the sidebar
          scroll twice. */}
      <div className="flex min-h-0 flex-1 flex-col">
        {(origin.kind === `board` || origin.kind === `issue`) && (
          <BoardListNav teamSlug={teamSlug} boardSlug={origin.boardSlug} />
        )}
        {origin.kind === `inbox` && (
          <InboxListNav teamSlug={teamSlug} tab={origin.tab ?? null} />
        )}
        {origin.kind === `support` && team && (
          <SupportListNav teamId={team.id} teamSlug={teamSlug} />
        )}
        {origin.kind === `reviews` && (
          <ReviewsListNav teamSlug={teamSlug} team={team} />
        )}
        {origin.kind === `agent` && team && <AgentListNav teamId={team.id} />}
      </div>
    </>
  )
}

/** The open detail's identity, off the route params — which row is active. */
function useActiveDetail(): {
  issueIdentifier: string | null
  sessionId: string | null
  threadId: string | null
} {
  const params = useParams({ strict: false }) as {
    issueIdentifier?: string
    sessionId?: string
    threadId?: string
  }
  return {
    issueIdentifier: params.issueIdentifier ?? null,
    sessionId: params.sessionId ?? null,
    threadId: params.threadId ?? null,
  }
}

/** The board's issue rows — the same pane the issue page used to carry on its
 *  left, now in the sidebar where one list belongs. */
function BoardListNav({
  teamSlug,
  boardSlug,
}: {
  teamSlug: string
  boardSlug: string
}) {
  const { visibleGroups, boardReady, board } = useBoardViewData({
    filters: emptyFilters,
    boardSlug,
    teamSlug,
  })
  const { issueIdentifier } = useActiveDetail()
  const activeIssueId = useMemo(() => {
    if (!issueIdentifier) return ``
    for (const group of visibleGroups) {
      const match = group.issues.find(
        (issue) => issue.identifier === issueIdentifier
      )
      if (match) return match.id
    }
    return ``
  }, [visibleGroups, issueIdentifier])

  if (!board) {
    return (
      <div className="px-3 py-2 text-xs text-muted-foreground">
        {boardReady ? `Board not found.` : `Loading…`}
      </div>
    )
  }
  return (
    <BoardIssueListPane
      groups={visibleGroups}
      teamSlug={teamSlug}
      boardSlug={boardSlug}
      activeIssueId={activeIssueId}
      filterSearch={{}}
      from={`board:${boardSlug}`}
    />
  )
}

/** The Inbox / My issues strip over the rows — the same control the big list
 *  view carries, at the same size (EXP-851 §E). The strip switches WHICH rows
 *  the nav lists; the rows hand the detail the matching token. */
function InboxListNav({
  teamSlug,
  tab,
}: {
  teamSlug: string
  tab: `my-issues` | null
}) {
  const [active, setActive] = useState<`inbox` | `my-issues`>(
    tab === `my-issues` ? `my-issues` : `inbox`
  )
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className={SEGMENTED_ROW_COMPACT}>
        <Tabs
          value={active}
          onValueChange={(next) => setActive(next as `inbox` | `my-issues`)}
          className="w-fit shrink-0"
        >
          <TabsList>
            <TabsTrigger value="inbox" className={SEGMENTED_TAB}>
              <InboxTabIcon />
              Inbox
            </TabsTrigger>
            <TabsTrigger value="my-issues" className={SEGMENTED_TAB}>
              <MyIssuesTabIcon />
              My Issues
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </div>
      <div className="flex min-h-0 flex-1 flex-col">
        {active === `inbox` ? (
          <InboxNavRows teamSlug={teamSlug} />
        ) : (
          <MyIssuesNavRows teamSlug={teamSlug} />
        )}
      </div>
    </div>
  )
}

function InboxNavRows({ teamSlug }: { teamSlug: string }) {
  const { issueIdentifier } = useActiveDetail()
  // The stream groups by ISSUE, and the route knows only the identifier — the
  // pane highlights by id, so the match rides the row's own identifier below.
  return (
    <InboxView
      teamSlug={teamSlug}
      compact
      activeIssueIdentifier={issueIdentifier}
      from="inbox"
    />
  )
}

function MyIssuesNavRows({ teamSlug }: { teamSlug: string }) {
  const { data: session } = useSession()
  const { visibleGroups, boardMap } = useMyIssuesData({
    filters: emptyFilters,
    userId: session?.user?.id,
    teamSlug,
  })
  const { issueIdentifier } = useActiveDetail()
  const boardSlugById = useMemo(
    () =>
      new Map(
        [...boardMap.entries()].map(([id, board]) => [id, board.slug] as const)
      ),
    [boardMap]
  )
  const activeIssueId = useMemo(() => {
    if (!issueIdentifier) return ``
    for (const group of visibleGroups) {
      const match = group.issues.find(
        (issue) => issue.identifier === issueIdentifier
      )
      if (match) return match.id
    }
    return ``
  }, [visibleGroups, issueIdentifier])
  return (
    <BoardIssueListPane
      groups={visibleGroups}
      teamSlug={teamSlug}
      // Cross-board: every row resolves its own board, this is the fallback.
      boardSlug={``}
      boardSlugById={boardSlugById}
      activeIssueId={activeIssueId}
      filterSearch={{}}
      from="inbox:my-issues"
    />
  )
}

function SupportListNav({
  teamId,
  teamSlug,
}: {
  teamId: string
  teamSlug: string
}) {
  const { threadId } = useActiveDetail()
  return (
    <SupportThreadList
      teamId={teamId}
      teamSlug={teamSlug}
      activeThreadId={threadId}
      compact
    />
  )
}

function AgentListNav({ teamId }: { teamId: string }) {
  const { data: session } = useSession()
  const { sessionId } = useActiveDetail()
  const currentUserId = session?.user?.id
  if (!currentUserId) return null
  return (
    <SessionsList
      teamId={teamId}
      currentUserId={currentUserId}
      activeSessionId={sessionId}
      origin={{ kind: `agent` }}
    />
  )
}

/** The review queue, one row per open pull request — the board groups the big
 *  list uses, minus the merge controls (those live on the detail). */
function ReviewsListNav({
  teamSlug,
  team,
}: {
  teamSlug: string
  team: Team | null | undefined
}) {
  const { groups, isLoading } = useReviewsData(team)
  const { issueIdentifier } = useActiveDetail()
  if (isLoading) {
    return <div className="px-3 py-2 text-xs text-muted-foreground">Loading…</div>
  }
  if (groups.length === 0) {
    return (
      <div className="px-3 py-2 text-xs text-muted-foreground">
        No open pull requests.
      </div>
    )
  }
  return (
    <div className="flex-1 overflow-y-auto p-2">
      {groups.map((group) => (
        <div key={group.board.id} className="mb-2">
          <GlassSectionHeader label={group.board.name} />
          <div className="flex flex-col">
            {group.entries.map((entry) => (
              <ListRow
                key={entry.key}
                asChild
                interactive
                active={entry.issue.identifier === issueIdentifier}
                className="gap-2"
              >
                <Link
                  to="/t/$teamSlug/reviews/$issueIdentifier"
                  params={{ teamSlug, issueIdentifier: entry.issue.identifier }}
                  search={{ from: `reviews` }}
                >
                  <GitPullRequest className="size-3.5 shrink-0 text-emerald-500" />
                  <span className="shrink-0 font-mono text-xs text-muted-foreground">
                    {entry.issues.length > 1 && entry.issue.prNumber
                      ? `#${entry.issue.prNumber}`
                      : entry.issue.identifier}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {entry.issues.length > 1
                      ? `${entry.issues.length} issues`
                      : entry.issue.title}
                  </span>
                </Link>
              </ListRow>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
