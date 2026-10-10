import { useEffect, useMemo, useState } from "react"
import { useParams } from "@tanstack/react-router"
import type { Label, Team, User } from "@/db/schema"
import type { IssueGroup } from "@/lib/board-view"
import { useMyIssuesData } from "@/hooks/use-my-issues-data"
import { useSession } from "@/hooks/use-session"
import {
  Button,
  conceptIcon,
  SEGMENTED_ROW_COMPACT,
  SegmentedControl,
} from "@exp/ui"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import { trpc } from "@/lib/trpc-client"
import { BoardIssueListPane } from "@/components/board-issue-list-pane"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { InboxView } from "@/components/inbox/inbox-view"

// EXP-851 / EXP-1246: the INBOX's list pane — the one list the list-detail
// host (`list-detail.tsx`) keeps beside an issue besides Agent › Recent. The
// board, reviews and action-runs panes are gone: those origins name Back only
// (`originHasListNav`). Every row navigates to ITS detail carrying the same
// `?from=` token, so the pane survives walking the list one detail at a time.

const InboxTabIcon = conceptIcon(`nav-inbox`)
const MyIssuesTabIcon = conceptIcon(`ui-assignee`)
const MarkReadIcon = conceptIcon(`notification-mark-read`)

/** "Mark all read" = the strip's trailing ghost glyph (desktop
 *  `sidebar.rs`'s Inbox strip), present only while something is unread. */
function MarkAllReadGlyph() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      className="shrink-0"
      aria-label="Mark all read"
      title="Mark all read"
      onClick={() => void trpc.notifications.markAllRead.mutate()}
    >
      <MarkReadIcon />
    </Button>
  )
}

/** The open detail's identity, off the route params — which row is active. */
function useActiveDetail(): {
  issueIdentifier: string | null
  sessionId: string | null
} {
  const params = useParams({ strict: false }) as {
    issueIdentifier?: string
    sessionId?: string
  }
  return {
    issueIdentifier: params.issueIdentifier ?? null,
    sessionId: params.sessionId ?? null,
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

/** The Inbox / My issues strip over the rows — the same control the big list
 *  view carries, at the same size (EXP-851 §E). The strip switches WHICH rows
 *  the nav lists; the rows hand the detail the matching token. */
export function InboxListNav({
  teamSlug,
  tab,
  onTabChange,
}: {
  teamSlug: string
  tab: `my-issues` | null
  /** EXP-1246: on the Inbox page itself the strip IS the page's `?tab=` — the
   *  host passes the navigation. Beside a detail it is local state. */
  onTabChange?: (next: `inbox` | `my-issues`) => void
}) {
  const [active, setActive] = useState<`inbox` | `my-issues`>(
    tab === `my-issues` ? `my-issues` : `inbox`
  )
  // The host stays mounted across inbox ↔ detail, so a new `tab` (the Inbox
  // page's URL, Back to My issues) must win over the local pick.
  useEffect(() => {
    setActive(tab === `my-issues` ? `my-issues` : `inbox`)
  }, [tab])
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className={SEGMENTED_ROW_COMPACT}>
        <SegmentedControl
          value={active}
          onValueChange={(next) => {
            setActive(next)
            onTabChange?.(next)
          }}
          options={[
            { value: `inbox`, label: `Inbox`, icon: InboxTabIcon },
            { value: `my-issues`, label: `My issues`, icon: MyIssuesTabIcon },
          ]}
        />
        {active === `inbox` && <MarkAllReadGlyph />}
      </div>
      <div className="flex min-h-0 flex-1 flex-col">
        {active === `inbox` ? (
          <InboxNavRows />
        ) : (
          <MyIssuesNavRows teamSlug={teamSlug} />
        )}
      </div>
    </div>
  )
}

function InboxNavRows() {
  const { issueIdentifier, sessionId } = useActiveDetail()
  // The stream groups by ISSUE, and the route knows only the identifier — the
  // pane highlights by id, so the match rides the row's own identifier below.
  // A message row that opened a run highlights off the run's id.
  return (
    <InboxView
      compact
      activeIssueIdentifier={issueIdentifier}
      activeSessionId={sessionId}
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
