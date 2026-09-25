import { useEffect, useMemo, useState } from "react"
import { Link, useNavigate, useParams } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { GitPullRequest } from "lucide-react"
import type { Board, CodingSession, Label, Team, User } from "@/db/schema"
import type { IssueGroup } from "@/lib/board-view"
import {
  originBoardSlug,
  originLabel,
  originListNavigation,
  type DetailOrigin,
} from "@/lib/detail-origin"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { useMyIssuesData } from "@/hooks/use-my-issues-data"
import { useReviewsData } from "@/hooks/use-reviews-data"
import { useSession } from "@/hooks/use-session"
import { useOpenSession } from "@/hooks/use-open-session"
import { codingSessionCollection } from "@/lib/collections"
import { useSessionListRows } from "@/hooks/use-agents-data"
import { SessionTree } from "@/components/session-tree"
import {
  conceptIcon,
  GlassSectionHeader,
  ListRow,
  SEGMENTED_ROW_COMPACT,
  SegmentedControl,
} from "@exp/ui"
import { BoardIssueListPane } from "@/components/board-issue-list-pane"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { InboxView } from "@/components/inbox/inbox-view"
import { SupportThreadList } from "@/components/helpdesk/support-inbox"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"

// EXP-851: the sidebar's list panel — the list a detail came from. EXP-870:
// it sits in the 17rem panel slot beside the compact rail (never replacing
// it), sharing that slot and its directional slide with the settings nav. It carries the same back row (`SidebarBackRow`, labelled
// with the list) and the list itself, simplified: a board's issue rows, the
// inbox stream, the support threads, the automated runs, the review queue.
// EXP-923: the AGENT origin lost its panel with the Agent page's own list —
// a run opened from there keeps the main menu (`originHasListNav`), and the
// page's Recent list is a toggled panel of its own (`recent-runs-nav.tsx`).
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
  const boardSlug = originBoardSlug(origin)
  const board = boardSlug
    ? (boards?.find((row) => row.slug === boardSlug) ?? null)
    : null

  // EXP-870: the one back-to-the-list destination (`originListNavigation`),
  // shared with the session route's Back and the md+ back chevron.
  const goBack = () => {
    const target = originListNavigation(teamSlug, origin)
    if (target) void navigate(target as never)
  }

  return (
    <>
      <SidebarBackRow label={originLabel(origin, board?.name)} onBack={goBack} />
      {/* A plain column, not `SidebarContent`: each list below owns its own
          scrollport, and two nested `overflow-auto` boxes make the sidebar
          scroll twice. */}
      <div className="flex min-h-0 flex-1 flex-col">
        {origin.kind === `board` && (
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
        {origin.kind === `automations` && team && (
          <AutomationsListNav team={team} />
        )}
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

/** EXP-996/EXP-1048: the sidebar's issue pane WITH the multiselect the IDE's
 *  own list column has (`sidebar.rs` `nav_*`). The selection lives here — the
 *  board page's model exactly: the host owns the ids, the list owns the
 *  gestures, and the bulk bar renders outside the pane's scrollport. Its
 *  "Start coding" IS the run entry point the selection feeds; no new copy. */
function IssueNavPane({
  groups,
  teamSlug,
  boardSlug,
  boardSlugById,
  activeIssueId,
  from,
  team,
  issueLabelMap,
  labels,
  users,
  listKey,
}: {
  groups: IssueGroup[]
  teamSlug: string
  boardSlug: string
  boardSlugById?: Map<string, string>
  activeIssueId: string
  from: string
  team: Team | null | undefined
  issueLabelMap: Map<string, Label[]>
  labels: Label[]
  users: User[]
  /** WHICH list this is. The selection is per list: a new occupant starts
   *  empty (the IDE clears it on every origin change). */
  listKey: string
}) {
  const permissions = useTeamPermissions(team)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  useEffect(() => {
    setSelectedIds(new Set())
  }, [listKey])
  const selectedIssues = useMemo(
    () =>
      groups
        .flatMap((group) => group.issues)
        .filter((issue) => selectedIds.has(issue.id)),
    [groups, selectedIds]
  )
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <BoardIssueListPane
        groups={groups}
        teamSlug={teamSlug}
        boardSlug={boardSlug}
        boardSlugById={boardSlugById}
        activeIssueId={activeIssueId}
        from={from}
        bulkTeamId={team?.id}
        canModerate={permissions.isModerator}
        selectedIds={selectedIds}
        onSelectedIdsChange={setSelectedIds}
      />
      {team && selectedIssues.length > 0 && (
        // EXP-289's no-jump rule: the bar FLOATS over the bottom of the rows,
        // which never move for it. The strip itself is click-through, so the
        // rows beside the capsule stay reachable.
        <div className="pointer-events-none absolute inset-x-0 bottom-2 z-10 flex justify-center px-2">
          <div className="pointer-events-auto">
            {/* 17rem of column: glyphs only, folding onto a second line. */}
            <BulkActionBar
              issues={selectedIssues}
              issueLabelMap={issueLabelMap}
              labels={labels}
              users={users}
              teamId={team.id}
              onClear={() => setSelectedIds(new Set())}
              iconOnly
              wrap
            />
          </div>
        </div>
      )}
    </div>
  )
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
  const {
    visibleGroups,
    boardReady,
    board,
    team,
    issueLabelMap,
    labelList,
    users,
  } = useBoardViewData({ boardSlug, teamSlug })
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
    <IssueNavPane
      groups={visibleGroups}
      teamSlug={teamSlug}
      boardSlug={boardSlug}
      activeIssueId={activeIssueId}
      from={`board:${boardSlug}`}
      team={team}
      issueLabelMap={issueLabelMap}
      labels={labelList}
      users={users}
      listKey={`board:${boardSlug}`}
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
        <SegmentedControl
          value={active}
          onValueChange={setActive}
          options={[
            { value: `inbox`, label: `Inbox`, icon: InboxTabIcon },
            { value: `my-issues`, label: `My Issues`, icon: MyIssuesTabIcon },
          ]}
        />
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
  const { visibleGroups, boardMap, team, issueLabelMap, labelList, users } =
    useMyIssuesData({ userId: session?.user?.id, teamSlug })
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
    <IssueNavPane
      groups={visibleGroups}
      teamSlug={teamSlug}
      // Cross-board: every row resolves its own board, this is the fallback.
      boardSlug={``}
      boardSlugById={boardSlugById}
      activeIssueId={activeIssueId}
      from="inbox:my-issues"
      team={team}
      issueLabelMap={issueLabelMap}
      labels={labelList}
      users={users}
      listKey={`inbox:my-issues`}
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

/** EXP-862: the Automations page's AUTOMATED runs — the one list that shows
 *  an unattended run. A finished automated run opened from that page keeps
 *  this list beside it, and Back returns to Automations (never the Agent
 *  page, whose list is the person-started one). Same rows the page's "Recent
 *  automated runs" section draws, at the sidebar's density. */
function AutomationsListNav({ team }: { team: Team }) {
  const teamId = team.id
  const { sessionId } = useActiveDetail()
  const openSession = useOpenSession()
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ sessions: codingSessionCollection })
        .where(({ sessions }) => eq(sessions.teamId, teamId)),
    [teamId]
  )
  // A run is AUTOMATED exactly when it carries a `started_reason` — set only
  // by the device-side automation hosts (the Automations page's own rule).
  const runs = useMemo(
    () =>
      [...((sessionRows ?? []) as CodingSession[])]
        .filter((session) => session.startedReason !== null)
        .sort(
          (left, right) =>
            new Date(right.createdAt).getTime() -
            new Date(left.createdAt).getTime()
        ),
    [sessionRows]
  )
  const rows = useSessionListRows(teamId, runs)
  if (rows.length === 0) {
    return (
      <div className="px-3 py-2 text-xs text-muted-foreground">
        Nothing has fired yet.
      </div>
    )
  }
  return (
    <div className="flex-1 overflow-y-auto p-2">
      {/* EXP-897: nested, like every other session list. */}
      <SessionTree
        rows={rows}
        activeSessionId={sessionId}
        onOpen={(session) =>
          openSession(session, { origin: { kind: `automations` } })
        }
      />
    </div>
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
                className="h-7 gap-2 px-2 py-0"
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
