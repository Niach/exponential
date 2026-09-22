import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue, Label, Board, User } from "@/db/schema"
import { boardCollection } from "@/lib/collections"
import { StatusDropdown } from "@/components/issue-properties/status-dropdown"
import { IssueGroupHeader } from "@/components/issue-group-header"
import { PriorityDropdown } from "@/components/issue-properties/priority-dropdown"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import { IssueRowContextMenu } from "@/components/issue-row-menu/context-menu"
import {
  EmptyState,
  Button,
  Pill,
  Checkbox,
  Skeleton,
  UserAvatar,
  useIsMobile,
  conceptIcon,
} from "@exp/ui"
import { Collapsible as CollapsiblePrimitive } from "radix-ui"
import { trpc } from "@/lib/trpc-client"
import { formatDate } from "@/lib/utils"
import { useToday } from "@/hooks/use-now"
import { dueDateToneClass } from "@/lib/issue-due-date"
import type { StatusRowOption } from "@/lib/team-statuses"
import type { IssueGroup } from "@/lib/board-view"
import type { BlockCounts } from "@/lib/issue-graph"
import { IssueBlocksBadge } from "@/components/issue-blocks-badge"
import {
  issueRail,
  railRowIsEmpty,
  railWidth,
  type IssueRail,
  type RailEntry,
  type RailRow,
} from "@/lib/issue-rail"
import {
  IssueRailGap,
  IssueRailLayer,
  RAIL_ROOT_ATTR,
} from "@/components/issue-rail"
import type { TeamIssueGraph } from "@/hooks/use-team-issue-graph"
import {
  TREE_INDENT,
  TreeGuides,
  treeGuideIsEmpty,
  treeGuides,
  type TreeGuide,
} from "@exp/ui"

// ×4 concepts, never raw glyphs (CLAUDE.md §Icons): the iOS list names the
// same ones (`IssueListView.swift`).
const AddIcon = conceptIcon(`ui-add`)
const DueDateIcon = conceptIcon(`ui-due-date`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const EmptyIssuesIcon = conceptIcon(`ui-checklist`)
const AvatarPlaceholderIcon = conceptIcon(`ui-avatar-placeholder`)

// REV-46: the desktop IDE virtualizes this exact list (issue_list.rs
// v_virtual_list — "the list can be long; virtualization is mandatory"). The
// web analog follows @exp/ui file-diff-card.tsx instead: cap + expand, so a board with
// thousands of issues never mounts thousands of interactive rows (each row is
// a Radix context menu around three dropdown components) in one commit.
// Each group renders this many rows before a "Show more" button takes over.
/** EXP-980: nudges the first gutter's centre under the parent's priority glyph
 *  (a 24px column, glyph centred at 12; a gutter centre sits at 7). */
const GUIDE_INSET = 5
const NO_COUNTS: ReadonlyMap<string, BlockCounts> = new Map()
const NO_DEPTHS: readonly number[] = []
const NO_GUIDE_KEYS: readonly string[] = []
const NO_RAIL: IssueRail = { entries: [], laneCount: 0, hasNodes: false }
/** EXP-998: the rail sits in the row's right padding column (`md:px-6`). */
const RAIL_LAYER_CLASS = `right-6 max-md:hidden`

const GROUP_ROW_CAP = 100
// Every "Show more" click reveals this many additional rows.
const GROUP_ROW_CHUNK = 400

interface IssueListProps {
  groups: IssueGroup[]
  issueLabelMap: Map<string, Label[]>
  labels: Label[]
  users: User[]
  userMap: Map<string, User>
  onNewIssue: (status?: StatusRowOption) => void
  onIssueClick: (issue: Issue) => void
  canCreate?: boolean
  canMutateIssue?: (issue: Issue) => boolean
  // Moderator-only row controls (status, priority, assignee, due date) are
  // disabled when false. Title/description/labels remain mutable by anyone
  // whose canMutateIssue is true.
  canModerate?: boolean
  // True while the Electric issues collection is still loading its first
  // snapshot — renders skeleton rows instead of an empty state.
  isLoading?: boolean
  // Rendered below the "No issues yet" empty state — the board passes the
  // member-only "Getting started" cards here (EXP-88).
  emptyStateExtra?: React.ReactNode
  // Optional trailing per-row action cell. Rendered in its own
  // click-isolated grid column. Rows are memoized (REV-46), so the output
  // must be a function of the issue row alone — external state it closes
  // over won't re-render untouched rows.
  renderRowAction?: (issue: Issue) => React.ReactNode
  // EXP-980/998: the team's relation graph (`useTeamIssueGraph`), queried
  // once per list: the phone badge's counts and the md+ rail's edges.
  issueGraph?: TeamIssueGraph
  // The team the graph (and the mini-graph behind a node) belongs to.
  graphTeamId?: string
  // Enables bulk selection (hover checkboxes on md+, shift-click ranges,
  // Cmd/Ctrl+A, Esc). Undefined = bulk select off. Selection also requires
  // canModerate.
  bulkTeamId?: string
  // Selection state is owned by the parent so it can render the BulkActionBar
  // in the header region above the scroll container (EXP-251). The setter is
  // a state dispatcher because the prune effect and keyboard handlers rely on
  // functional updates.
  selectedIds?: Set<string>
  onSelectedIdsChange?: React.Dispatch<React.SetStateAction<Set<string>>>
}

const EMPTY_SELECTION = new Set<string>()
const noopSetSelectedIds: React.Dispatch<React.SetStateAction<Set<string>>> =
  () => {}
// Shared empty-labels instance — a fresh `[]` per label-less row would break
// the row memo on every parent render.
const NO_LABELS: Label[] = []

function IssueListSkeleton() {
  return (
    <div
      data-testid="issue-list-skeleton"
      className="max-md:flex max-md:flex-col max-md:gap-[3px] max-md:px-4 max-md:pt-1"
    >
      <div className="flex items-center gap-2 max-md:px-2 max-md:py-2 md:pl-3 md:pr-6 md:py-2 md:border-b md:border-border/50 md:bg-accent/20">
        <Skeleton className="h-3.5 w-3.5 rounded-full" />
        <Skeleton className="h-3.5 w-24" />
      </div>
      {Array.from({ length: 5 }, (_, i) => (
        <div
          key={i}
          className="flex items-center gap-3 h-12 md:h-10 px-3 md:px-6 md:border-b md:border-border/30 max-md:rounded-md max-md:border max-md:border-glass-stroke max-md:bg-glass-row"
        >
          <Skeleton className="h-4 w-4 rounded-full" />
          <Skeleton className="h-3 w-14" />
          <Skeleton className="h-4 w-4 rounded-full" />
          <Skeleton className="h-3.5 flex-1 max-w-72" />
        </div>
      ))}
    </div>
  )
}

// The row's 20px avatar cell (EXP-941 folded the old `AssigneeDropdown` into
// the shared `AssigneePicker`): a team with no one else to assign to — or a
// row the viewer may not mutate — gets the same avatar as a STATIC cell
// rather than a pointless dropdown.
function AssigneeCell({
  issue,
  users,
  userMap,
  canMutate,
}: {
  issue: Issue
  users: User[]
  userMap: Map<string, User>
  canMutate: boolean
}) {
  const assignee = issue.assigneeId ? userMap.get(issue.assigneeId) : undefined
  const avatar = assignee ? (
    <UserAvatar size={20} user={assignee} />
  ) : (
    <div className="size-5 rounded-full border border-dashed border-border flex items-center justify-center">
      <AvatarPlaceholderIcon className="size-2.5 text-muted-foreground/50" />
    </div>
  )

  if (!canMutate) {
    return (
      <div className="flex h-5 w-5 items-center justify-center">{avatar}</div>
    )
  }

  return (
    <AssigneePicker
      users={users}
      selectedUserId={issue.assigneeId}
      onSelect={(userId) => {
        void trpc.issues.update.mutate({ id: issue.id, assigneeId: userId })
      }}
      align="end"
      trigger={
        <Button variant="ghost" className="h-5 w-5 p-0">
          {avatar}
        </Button>
      }
    />
  )
}

interface IssueRowProps {
  issue: Issue
  issueLabels: Label[]
  labels: Label[]
  users: User[]
  userMap: Map<string, User>
  teamBoards?: Board[]
  rowGridClass: string
  today: string
  isSolo: boolean
  bulkEnabled: boolean
  mobileSelectionActive: boolean
  isSelected: boolean
  // Any selection exists — keeps every row's checkbox visible (not just
  // hovered) while a selection is in progress.
  anySelected: boolean
  // rowCanMutate && canModerate, precomputed so the memo compares a boolean.
  canMutateRow: boolean
  onOpen: (issue: Issue) => void
  onToggleSelect: (issueId: string, shiftKey: boolean) => void
  renderRowAction?: (issue: Issue) => React.ReactNode
  /** EXP-980: 0 = a root row; a sub-issue sits one level per ancestor in. */
  depth: number
  /** The row's `TreeGuide` as JSON, `` for none: a STRING so the memo still
   *  compares primitives. */
  guideKey: string
  /** EXP-980: open blockers / open blocked issues; both 0 = no badge. */
  blockedBy: number
  blocking: number
  /** The badge's graph scope. An issue row carries no team id itself. */
  graphTeamId: string | undefined
  /** EXP-998: the row's `RailRow` as JSON, `` for none — a STRING, so the
   *  memo still compares primitives. */
  railKey: string
  /** The rail column's width; 0 = no rail in this list. */
  railWidth: number
}

// REV-46: memoized so a selection toggle reconciles only the toggled row —
// `selectedIds` lives in the route, so every checkbox click re-renders the
// whole page; without the memo each click re-rendered every row's context
// menu + three dropdowns. All props are primitives or referentially stable
// (the callbacks come from the latest-ref wrappers in IssueList).
const IssueRow = memo(function IssueRow({
  issue,
  issueLabels,
  labels,
  users,
  userMap,
  teamBoards,
  rowGridClass,
  today,
  isSolo,
  bulkEnabled,
  mobileSelectionActive,
  isSelected,
  anySelected,
  canMutateRow,
  onOpen,
  onToggleSelect,
  renderRowAction,
  depth,
  guideKey,
  blockedBy,
  blocking,
  graphTeamId,
  railKey,
  railWidth: rail,
}: IssueRowProps) {
  const guide = useMemo(
    () => (guideKey ? (JSON.parse(guideKey) as TreeGuide) : null),
    [guideKey]
  )
  const railRow = useMemo(
    () => (railKey ? (JSON.parse(railKey) as RailRow) : null),
    [railKey]
  )
  const indent = depth * TREE_INDENT
  return (
    <IssueRowContextMenu
      issue={issue}
      issueLabels={issueLabels}
      labels={labels}
      users={users}
      userMap={userMap}
      boards={teamBoards}
      onOpenIssue={() => onOpen(issue)}
      onToggleSelect={
        bulkEnabled ? () => onToggleSelect(issue.id, false) : undefined
      }
      isSelected={isSelected}
    >
      <div
        // EXP-620: below md the row is a native-style glass CARD in a flex
        // line (mirroring the iOS HStack / Compose Row), at md+ the flush
        // table grid it has always been. Flex on mobile is what lets the
        // mobile-only cells below appear without a second set of grid
        // templates. The mobile fill is chosen here rather than by class
        // order: `bg-glass-active` and `bg-glass-row` are both `bg-*`
        // utilities, so which one won would come down to stylesheet order.
        className={`relative max-md:flex max-md:items-center max-md:gap-2.5 max-md:rounded-md max-md:border max-md:border-glass-stroke md:grid ${rowGridClass} items-center h-12 md:h-10 px-3 md:px-6 md:hover:bg-glass-row md:border-b md:border-border/30 group/row cursor-pointer ${isSelected ? `max-md:bg-glass-active max-md:border-glass-stroke-active` : `max-md:bg-glass-row`}`}
        onClick={() => {
          if (mobileSelectionActive) {
            onToggleSelect(issue.id, false)
            return
          }
          onOpen(issue)
        }}
        data-testid={`issue-row-${issue.identifier}`}
        data-depth={depth}
        style={
          indent > 0
            ? ({ "--issue-indent": `${indent}px` } as React.CSSProperties)
            : undefined
        }
      >
        {/* EXP-980: the elbow from the parent's priority glyph. The gutters
            start at the PRIORITY column (past the md+ checkbox column), its
            first centre under the parent's glyph; the indent itself is the
            priority cell's padding, so the checkbox never moves. */}
        {guide && (
          <>
            <TreeGuides
              guide={guide}
              base={(bulkEnabled ? 44 : 24) + GUIDE_INSET}
              className="max-md:hidden"
            />
            <TreeGuides
              guide={guide}
              base={12 + GUIDE_INSET}
              gap={3}
              className="md:hidden"
            />
          </>
        )}
        <IssueRailLayer
          row={railRow}
          width={rail}
          issueId={issue.id}
          teamId={graphTeamId}
          className={RAIL_LAYER_CLASS}
        />
        {bulkEnabled && (
          <div
            // self-stretch + the padding bleed grow the toggle
            // hitbox to the full row height and the row's left
            // padding — a click slightly beside the checkbox
            // must select, never open the issue (FEED-12).
            className="hidden md:flex items-center self-stretch -ml-6 pl-6"
            // Suppress the browser's shift-click text selection
            // so range-select doesn't highlight row text.
            onMouseDown={(e) => {
              if (e.shiftKey) e.preventDefault()
            }}
            onClick={(e) => {
              e.stopPropagation()
              onToggleSelect(issue.id, e.shiftKey)
            }}
          >
            <Checkbox
              checked={isSelected}
              aria-label={`Select ${issue.identifier}`}
              className={`transition-opacity ${anySelected ? `opacity-100` : `opacity-0 group-hover/row:opacity-100`}`}
            />
          </div>
        )}
        <div
          className="flex items-center justify-center max-md:shrink-0"
          style={indent > 0 ? { paddingLeft: indent } : undefined}
          onClick={(e) => e.stopPropagation()}
        >
          <PriorityDropdown
            issueId={issue.id}
            priority={issue.priority}
            disabled={!canMutateRow}
          />
        </div>
        {/* Native parity (EXP-620): the identifier shows on mobile too, on a
            min-width column so the status glyph and title line up across rows
            for typical digit counts without clipping longer identifiers. */}
        <span className="text-xs text-muted-foreground font-mono truncate max-md:min-w-[3.75rem] max-md:shrink-0">
          {issue.identifier}
        </span>
        <div
          className="flex items-center justify-center"
          onClick={(e) => e.stopPropagation()}
        >
          <StatusDropdown
            issueId={issue.id}
            status={issue.status}
            statusId={issue.statusId}
            disabled={!canMutateRow}
          />
        </div>
        <span className="flex items-center gap-1.5 text-sm truncate md:ml-2 min-w-0 max-md:flex-1">
          <span className="truncate">{issue.title}</span>
          {/* EXP-998: the counts pill is the PHONE's affordance (the natives'
              too); at md+ the rail at the row's right edge draws the
              relations as arrows and its node opens the same mini-graph. */}
          {(blockedBy > 0 || blocking > 0) && graphTeamId && (
            <IssueBlocksBadge
              issueId={issue.id}
              teamId={graphTeamId}
              blockedBy={blockedBy}
              blocking={blocking}
              className="md:hidden"
            />
          )}
        </span>
        {/* Full pills at md+; below md the natives draw up to three bare
            colour dots instead, which is all a phone-width row can hold. */}
        <div className="hidden md:flex items-center gap-1.5 ml-4 shrink-0">
          {issueLabels.map((label) => (
            <Pill key={label.id} dot={label.color}>
              {label.name}
            </Pill>
          ))}
        </div>
        {/* EXP-698: below `sm` the title gets the whole line — the dots and
            the due date are what truncated it to "Pus…". */}
        {issueLabels.length > 0 && (
          <div className="hidden sm:flex md:hidden items-center gap-1 shrink-0">
            {issueLabels.slice(0, 3).map((label) => (
              <div
                key={label.id}
                className="h-2 w-2 rounded-full shrink-0"
                style={{ backgroundColor: label.color }}
              />
            ))}
          </div>
        )}
        {/* Solo teams hide the avatar entirely on every client (`isSolo` here,
            `singleMemberTeam` on iOS, `soloMemberId` on Android). `order` puts
            it after the due date on mobile, matching the native row; at md+
            the grid keeps the established column sequence. */}
        {!isSolo && (
          <div
            className="flex items-center justify-center max-md:order-1"
            onClick={(e) => e.stopPropagation()}
          >
            <AssigneeCell
              issue={issue}
              users={users}
              userMap={userMap}
              canMutate={canMutateRow}
            />
          </div>
        )}
        {/* Display-only: due dates are edited in the issue
            detail, never inline from the list (EXP-247). The
            tone (red overdue / orange today) is what explains
            the overdue-first ordering — REV2-48. */}
        <div className="max-sm:hidden flex items-center justify-end">
          {issue.dueDate && (
            <span
              className={`flex items-center gap-1 px-1 ${dueDateToneClass(issue.dueDate, today)}`}
            >
              <DueDateIcon className="size-3 shrink-0" />
              <span className="text-xs whitespace-nowrap">
                {formatDate(issue.dueDate)}
              </span>
            </span>
          )}
        </div>
        {renderRowAction && (
          <div
            className="flex items-center justify-end max-md:order-2"
            onClick={(e) => e.stopPropagation()}
          >
            {renderRowAction(issue)}
          </div>
        )}
        {/* Both natives end the row with a disclosure chevron (Android draws
            one, iOS gets it from NavigationLink); desktop's table has no such
            affordance, so it stays mobile-only. */}
        <ChevronRightIcon className="size-4 shrink-0 text-muted-foreground md:hidden max-md:order-3" />
      </div>
    </IssueRowContextMenu>
  )
})

export function IssueList({
  groups,
  issueLabelMap,
  labels,
  users,
  userMap,
  onNewIssue,
  onIssueClick,
  canCreate = true,
  canMutateIssue,
  canModerate = true,
  isLoading = false,
  emptyStateExtra,
  renderRowAction,
  issueGraph,
  graphTeamId,
  bulkTeamId,
  selectedIds = EMPTY_SELECTION,
  onSelectedIdsChange: setSelectedIds = noopSetSelectedIds,
}: IssueListProps) {
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set())
  // Extra rows revealed per group id beyond GROUP_ROW_CAP via "Show more"
  // (REV-46). Only ever grows; a stale entry after a data change is just a
  // higher cap for that group.
  const [extraRows, setExtraRows] = useState<Map<string, number>>(new Map())
  // The shift-range anchor is never rendered — a ref keeps toggleSelect
  // referentially stable so it doesn't re-render every memoized row.
  const anchorIdRef = useRef<string | null>(null)
  const listRef = useRef<HTMLDivElement>(null)
  // Local-date boundary for the due-date tone — the same one the overdue-first
  // comparator sorts on (lib/board-view.ts). Ticks across midnight so a
  // long-lived tab keeps tone and ordering in agreement (REV2-48).
  const today = useToday()
  const visibleGroups = groups.filter((g) => g.issues.length > 0)
  const bulkEnabled = Boolean(bulkTeamId) && canModerate
  const isMobile = useIsMobile()
  // Mobile has no checkbox column: selection starts from the row context
  // menu's Select item, and while any selection exists a row TAP toggles it
  // instead of navigating (deselecting the last row exits — the native
  // EXP-405 contract). Desktop click-to-open is untouched.
  const mobileSelectionActive = isMobile && bulkEnabled && selectedIds.size > 0

  // Team boards feed the context menu's move-to-board submenu
  // (EXP-57). Trashed boards never reach the client (the boards shape
  // filters them server-side).
  const { data: boardRows } = useLiveQuery(
    (query) =>
      bulkTeamId
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) => eq(boards.teamId, bulkTeamId))
        : undefined,
    [bulkTeamId]
  )
  const teamBoards = useMemo(
    () =>
      bulkTeamId
        ? [...((boardRows ?? []) as Board[])].sort((left, right) =>
            left.name.localeCompare(right.name)
          )
        : undefined,
    [boardRows, bulkTeamId]
  )

  const renderLimit = (groupId: string) =>
    GROUP_ROW_CAP + (extraRows.get(groupId) ?? 0)

  // The range/select-all universe: RENDERED rows in render order — windowed
  // by the group cap, minus collapsed groups. Rows hidden behind "Show more"
  // stay out deliberately: a shift-range or Cmd/Ctrl+A must never sweep up
  // rows the user hasn't revealed.
  const visibleFlatIssues = useMemo(
    () =>
      groups
        .filter(
          (group) =>
            group.issues.length > 0 && !collapsedGroups.has(group.status.id)
        )
        .flatMap((group) =>
          group.issues.slice(0, renderLimit(group.status.id))
        ),
    [groups, collapsedGroups, extraRows]
  )
  const visibleFlatIssuesRef = useRef(visibleFlatIssues)
  visibleFlatIssuesRef.current = visibleFlatIssues

  // Prune selected ids whose rows left the data set (delete
  // elsewhere, sync). Collapsing a group hides rows but keeps them selected.
  useEffect(() => {
    const present = new Set(
      groups.flatMap((group) => group.issues.map((issue) => issue.id))
    )
    setSelectedIds((prev) => {
      const next = new Set([...prev].filter((id) => present.has(id)))
      return next.size === prev.size ? prev : next
    })
  }, [groups])

  // An emptied selection (bulk-bar Clear, Esc, external reset) also drops
  // the shift-range anchor — the next shift-click must not extend a range
  // from a pre-clear row.
  useEffect(() => {
    if (selectedIds.size === 0) anchorIdRef.current = null
  }, [selectedIds])

  // Stable (reads the universe + anchor through refs) so memoized rows don't
  // re-render when the selection changes.
  const toggleSelect = useCallback(
    (issueId: string, shiftKey: boolean) => {
      const ids = visibleFlatIssuesRef.current.map((issue) => issue.id)
      const anchorId = anchorIdRef.current
      const anchorIndex = anchorId ? ids.indexOf(anchorId) : -1
      const targetIndex = ids.indexOf(issueId)
      if (shiftKey && anchorIndex !== -1 && targetIndex !== -1) {
        // Shift-click extends: ADD the contiguous visible slice between the
        // anchor and the target (anchor stays put for further extensions).
        const [from, to] =
          anchorIndex < targetIndex
            ? [anchorIndex, targetIndex]
            : [targetIndex, anchorIndex]
        const range = ids.slice(from, to + 1)
        setSelectedIds((prev) => {
          const next = new Set(prev)
          for (const id of range) next.add(id)
          return next
        })
        return
      }
      setSelectedIds((prev) => {
        const next = new Set(prev)
        if (next.has(issueId)) {
          next.delete(issueId)
        } else {
          next.add(issueId)
        }
        return next
      })
      anchorIdRef.current = issueId
    },
    [setSelectedIds]
  )

  // Latest-ref wrappers: the callers hold the selection state, so their
  // inline callbacks get a new identity on every selection change — routing
  // the calls through refs keeps the row props stable.
  const onIssueClickRef = useRef(onIssueClick)
  onIssueClickRef.current = onIssueClick
  const openIssue = useCallback(
    (issue: Issue) => onIssueClickRef.current(issue),
    []
  )
  const renderRowActionRef = useRef(renderRowAction)
  renderRowActionRef.current = renderRowAction
  const hasRowAction = Boolean(renderRowAction)
  const stableRenderRowAction = useMemo(
    () =>
      hasRowAction
        ? (issue: Issue) => renderRowActionRef.current?.(issue)
        : undefined,
    [hasRowAction]
  )

  // Cmd/Ctrl+A selects every visible row; Escape
  // clears. Both keys are overlay-scoped: an Escape that dismisses a Radix
  // menu/dialog/popover must NOT also wipe the selection (Linear closes only
  // the menu), and select-all only fires with focus on the body or inside
  // the list — never while an overlay is up or a field elsewhere has focus.
  // Radix flushes its close via React batching AFTER this event finishes, so
  // querying open overlays during the bubble phase still sees them.
  useEffect(() => {
    if (!bulkEnabled) return
    const overlayOpen = () =>
      document.querySelector(
        `[data-state="open"][role="menu"], [data-state="open"][role="listbox"], [data-state="open"][role="dialog"]`
      ) !== null
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === `a`) {
        if (overlayOpen()) return
        const active = document.activeElement
        if (
          active instanceof HTMLElement &&
          (active instanceof HTMLInputElement ||
            active instanceof HTMLTextAreaElement ||
            active instanceof HTMLSelectElement ||
            active.isContentEditable)
        ) {
          return
        }
        if (
          active !== document.body &&
          active !== null &&
          !listRef.current?.contains(active)
        ) {
          return
        }
        event.preventDefault()
        setSelectedIds(new Set(visibleFlatIssues.map((issue) => issue.id)))
        return
      }
      if (event.key === `Escape`) {
        if (overlayOpen()) return
        setSelectedIds((prev) => (prev.size > 0 ? new Set<string>() : prev))
      }
    }
    window.addEventListener(`keydown`, handleKeyDown)
    return () => window.removeEventListener(`keydown`, handleKeyDown)
  }, [bulkEnabled, visibleFlatIssues])

  // Solo team (exactly one human member): the assignee cell is hidden
  // entirely (EXP-247 — nothing to reassign). `users` is the bot-excluded
  // member list; length 0 means still loading (never a genuine empty), so a
  // multi-member team never briefly reads as solo.
  const isSolo = users.length === 1

  // The row grid grows a leading checkbox column (md+ when bulk select is
  // on), drops the assignee column on solo teams, and grows a trailing
  // action column when the caller renders one. Every combination is a full
  // literal — Tailwind only sees complete class strings. md+ ONLY: below the
  // breakpoint the row is a flex card (EXP-620), so there is no mobile
  // template to keep in step here.
  const rowGridClass = bulkEnabled
    ? renderRowAction
      ? isSolo
        ? `md:grid-cols-[1.25rem_calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_4.5rem_2rem_var(--issue-rail,0px)]`
        : `md:grid-cols-[1.25rem_calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_2rem_var(--issue-rail,0px)]`
      : isSolo
        ? `md:grid-cols-[1.25rem_calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_4.5rem_var(--issue-rail,0px)]`
        : `md:grid-cols-[1.25rem_calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_var(--issue-rail,0px)]`
    : renderRowAction
      ? isSolo
        ? `md:grid-cols-[calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_4.5rem_2rem_var(--issue-rail,0px)]`
        : `md:grid-cols-[calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_2rem_var(--issue-rail,0px)]`
      : isSolo
        ? `md:grid-cols-[calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_4.5rem_var(--issue-rail,0px)]`
        : `md:grid-cols-[calc(1.5rem+var(--issue-indent,0px))_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem_var(--issue-rail,0px)]`

  // EXP-998: the blocks rail over the VISIBLE entries — every rendered row,
  // every group header (folded or not) and every "Show more" button, in
  // order — so an arrow between two rows crosses whatever sits between them
  // and stops short of a row that is folded away or behind the cap.
  const counts = issueGraph?.counts ?? NO_COUNTS
  const { rail, railAt } = useMemo(() => {
    if (!issueGraph) return { rail: NO_RAIL, railAt: new Map<string, number>() }
    const entries: RailEntry[] = []
    const at = new Map<string, number>()
    for (const group of visibleGroups) {
      at.set(`header:${group.status.id}`, entries.length)
      entries.push({ kind: `gap` })
      if (collapsedGroups.has(group.status.id)) continue
      const limit = GROUP_ROW_CAP + (extraRows.get(group.status.id) ?? 0)
      const rendered = group.issues.slice(0, limit)
      for (const issue of rendered) {
        at.set(issue.id, entries.length)
        entries.push({ kind: `row`, id: issue.id })
      }
      if (group.issues.length > rendered.length) {
        at.set(`more:${group.status.id}`, entries.length)
        entries.push({ kind: `gap` })
      }
    }
    return {
      rail: issueRail(entries, issueGraph.relations, issueGraph.issues),
      railAt: at,
    }
  }, [issueGraph, visibleGroups, collapsedGroups, extraRows])
  const railW = railWidth(rail)
  const railRowAt = (key: string): RailRow | undefined => {
    const index = railAt.get(key)
    return index === undefined ? undefined : rail.entries[index]
  }

  const toggleGroup = (groupKey: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev)
      if (next.has(groupKey)) {
        next.delete(groupKey)
      } else {
        next.add(groupKey)
      }
      return next
    })
  }

  if (visibleGroups.length === 0) {
    if (isLoading) {
      return <IssueListSkeleton />
    }

    return (
      <div>
        <EmptyState
          icon={EmptyIssuesIcon}
          title="No issues yet"
          description="Create an issue to start tracking work."
        >
          {canCreate && (
            <Button size="sm" onClick={() => onNewIssue()}>
              <AddIcon className="mr-1.5 size-4" />
              New issue
            </Button>
          )}
        </EmptyState>
        {emptyStateExtra}
      </div>
    )
  }

  return (
    // EXP-620: below md the list is the natives' 3dp-gapped card stack inside
    // a 16px gutter (the same gutter the header row uses above it); at md+ it
    // stays a flush, edge-to-edge table.
    <div
      ref={listRef}
      // EXP-998: `group/rail` + the root attribute = what the rail's hover
      // reveal and edge highlight key off; the column width is read by every
      // row's grid template.
      className="group/rail max-md:flex max-md:flex-col max-md:gap-[3px] max-md:px-4 max-md:pt-1"
      style={{ "--issue-rail": `${railW}px` } as React.CSSProperties}
      {...{ [RAIL_ROOT_ATTR]: `` }}
    >
      {visibleGroups.map((group) => {
        const option = group.status
        const isOpen = !collapsedGroups.has(option.id)
        const limit = renderLimit(option.id)
        const renderedIssues =
          group.issues.length > limit ? group.issues.slice(0, limit) : group.issues
        const hiddenCount = group.issues.length - renderedIssues.length
        // EXP-980: the guides of the rows actually RENDERED, so a capped
        // group never draws a tee towards a row behind "Show more".
        const depths = group.depths?.slice(0, renderedIssues.length) ?? NO_DEPTHS
        const guideKeys =
          depths.length > 0
            ? treeGuides(depths).map((guide) =>
                treeGuideIsEmpty(guide) ? `` : JSON.stringify(guide)
              )
            : NO_GUIDE_KEYS
        return (
          <CollapsiblePrimitive.Root
            key={option.id}
            open={isOpen}
            onOpenChange={() => toggleGroup(option.id)}
            className="max-md:flex max-md:flex-col max-md:gap-[3px]"
            data-testid={`issue-group-${option.id}`}
            // Stable across custom-status renames/ids: the builtin anchor key
            // for builtin groups, the row id otherwise. E2E selects on this.
            data-status-key={option.builtinKey ?? option.id}
          >
            {/* EXP-862: the ONE group band (`components/issue-group-header`)
                — the sidebar's issue lists draw the very same one. The whole
                strip folds its group, so the Collapsible root is driven from
                here instead of through a Trigger. EXP-620: a phone's header
                is plain text on the app background, no tint, matching
                iOS/Android (`useIsMobile` shares the 768px breakpoint with
                `md:`, so it agrees with the band's own classes). */}
            <IssueGroupHeader
              status={option}
              count={group.issues.length}
              open={isOpen}
              onToggle={() => toggleGroup(option.id)}
              tinted={!isMobile}
              overlay={
                <IssueRailGap
                  row={railRowAt(`header:${option.id}`)}
                  width={railW}
                  className={RAIL_LAYER_CLASS}
                />
              }
              trailing={
                canCreate ? (
                  <Button
                    variant="glass"
                    size="icon-sm"
                    aria-label={`New issue in ${option.name}`}
                    className="hidden md:inline-flex opacity-0 group-hover:opacity-100 hover:opacity-100"
                    onClick={(e) => {
                      e.stopPropagation()
                      // A new issue can never be born a duplicate (no
                      // canonical issue to pair with) — the duplicate group's
                      // "+" seeds nothing and the dialog falls back to
                      // Backlog.
                      onNewIssue(
                        option.category === `duplicate` ? undefined : option
                      )
                    }}
                  >
                    <AddIcon className="size-3" />
                  </Button>
                ) : undefined
              }
            />

            {/* Issue rows */}
            {/* The 3px row gap rides margins, NOT `flex`+`gap`: Radix hides a
                closed Content with the `hidden` ATTRIBUTE, and a `display`
                utility would out-specify the preflight `[hidden]` rule and
                leave collapsed groups showing their rows. */}
            <CollapsiblePrimitive.Content className="overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down max-md:space-y-[3px]">
              {renderedIssues.map((issue, index) => {
                const rowCanMutate = canMutateIssue
                  ? canMutateIssue(issue)
                  : true
                return (
                  <IssueRow
                    key={issue.id}
                    issue={issue}
                    issueLabels={issueLabelMap.get(issue.id) ?? NO_LABELS}
                    labels={labels}
                    users={users}
                    userMap={userMap}
                    teamBoards={teamBoards}
                    rowGridClass={rowGridClass}
                    today={today}
                    isSolo={isSolo}
                    bulkEnabled={bulkEnabled}
                    mobileSelectionActive={mobileSelectionActive}
                    isSelected={selectedIds.has(issue.id)}
                    anySelected={selectedIds.size > 0}
                    canMutateRow={rowCanMutate && canModerate}
                    onOpen={openIssue}
                    onToggleSelect={toggleSelect}
                    renderRowAction={stableRenderRowAction}
                    depth={depths[index] ?? 0}
                    guideKey={guideKeys[index] ?? ``}
                    blockedBy={counts.get(issue.id)?.blockedBy ?? 0}
                    blocking={counts.get(issue.id)?.blocking ?? 0}
                    graphTeamId={graphTeamId}
                    railKey={(() => {
                      const row = railRowAt(issue.id)
                      return railRowIsEmpty(row) ? `` : JSON.stringify(row)
                    })()}
                    railWidth={railW}
                  />
                )
              })}
              {hiddenCount > 0 && (
                // EXP-998: `relative`, so an arrow crossing the button (to a
                // row in a later group) keeps its line through it.
                <div className="relative">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="w-full text-xs text-muted-foreground md:rounded-none md:border-b md:border-border/30 max-md:rounded-md max-md:border max-md:border-glass-stroke max-md:bg-glass-row"
                    onClick={() =>
                      setExtraRows((prev) =>
                        new Map(prev).set(
                          option.id,
                          (prev.get(option.id) ?? 0) + GROUP_ROW_CHUNK
                        )
                      )
                    }
                  >
                    Show {Math.min(GROUP_ROW_CHUNK, hiddenCount)} more (
                    {hiddenCount} hidden)
                  </Button>
                  <IssueRailGap
                    row={railRowAt(`more:${option.id}`)}
                    width={railW}
                    className={RAIL_LAYER_CLASS}
                  />
                </div>
              )}
            </CollapsiblePrimitive.Content>
          </CollapsiblePrimitive.Root>
        )
      })}
    </div>
  )
}
