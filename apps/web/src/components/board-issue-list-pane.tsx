import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Link } from "@tanstack/react-router"
import type { IssueGroup } from "@/lib/board-view"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { IssueGroupHeader } from "@/components/issue-group-header"
import { issueMenuProps } from "@/components/issue-context-menu/attr"
import {
  Checkbox,
  ListRow,
  TREE_INDENT,
  TreeGuides,
  treeGuides,
  useIsMobile,
} from "@exp/ui"

// EXP-818 (the navigation rule, web): an issue opened with no list context of
// its own brings its BOARD along — so the issue page is a master-detail on md+,
// the board's list on the left exactly like the Agent page's sessions list and
// the inbox's stream. Compact by design: status glyph, identifier, title, one
// section per status ROW (EXP-314), the open issue highlighted. The big
// `IssueList`'s context menus and row actions stay the board page's.
//
// EXP-996/EXP-1048: the SELECTION is not — this pane carries the multiselect
// the IDE's own list column has had since EXP-863, with the same gestures and
// the same model as `IssueList` (hover checkbox, Cmd/Ctrl-click, Shift-range,
// Cmd/Ctrl+A, Esc; state owned by the host so the bulk bar can float over the
// rows). The bar's "Start coding" is the "Implement <issues>" entry point the
// issue asks for — no new copy.
//
// EXP-862: the section band IS the big list's group header
// (`IssueGroupHeader`, compact density) — same glyph, same name, same count
// over the same status tint, and it folds its group here too.
//
// Below md the issue is its own screen, as before — this pane never renders
// there, so a phone keeps the whole width for the issue.

/** The compact row's own left padding (`px-2`): the first gutter starts here,
 *  its centre under the parent's status glyph. */
const PANE_ROW_PAD = 8
/** EXP-1048: the leading select cell — a 16px box in the row's 8px padding
 *  plus the 4px before the status glyph, the IDE's `nav-select-cell`
 *  (`sidebar.rs`: `w_4` + `gap_1`). Reserved for the whole list while
 *  selection is on, so revealing a checkbox never moves a row. */
const SELECT_CELL = 20

const EMPTY_SELECTION = new Set<string>()
const noopSetSelectedIds: React.Dispatch<React.SetStateAction<Set<string>>> =
  () => {}

export function BoardIssueListPane({
  groups,
  teamSlug,
  boardSlug,
  boardSlugById,
  activeIssueId,
  from,
  canModerate,
  bulkTeamId,
  selectedIds = EMPTY_SELECTION,
  onSelectedIdsChange: setSelectedIds = noopSetSelectedIds,
}: {
  groups: IssueGroup[]
  teamSlug: string
  /** The board every row belongs to — the single-board case. */
  boardSlug: string
  /** EXP-851: a CROSS-BOARD list (the sidebar's My issues nav) resolves each
   * row's board here; `boardSlug` is the fallback. */
  boardSlugById?: Map<string, string>
  activeIssueId: string
  /** EXP-851: the `?from=` token every row hands the issue it opens, so the
   * detail keeps THIS list beside it (`lib/detail-origin.ts`). */
  from?: string
  /** Selection also requires moderation — `IssueList`'s gate, same name. */
  canModerate?: boolean
  /** EXP-1048: enables bulk selection. Undefined = selection off. */
  bulkTeamId?: string
  /** Owned by the host (`list-nav.tsx`), like the board page owns the big
   *  list's — it renders the bulk bar outside this scrollport. The setter is
   *  a dispatcher: the prune and the keyboard handlers update functionally. */
  selectedIds?: Set<string>
  onSelectedIdsChange?: React.Dispatch<React.SetStateAction<Set<string>>>
}) {
  // Folded groups, per status row, for as long as the pane is mounted — the
  // big list's rule (`IssueList`'s `collapsedGroups`).
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  const toggle = (statusId: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(statusId)) next.add(statusId)
      return next
    })
  const visible = groups.filter((group) => group.issues.length > 0)
  const paneRef = useRef<HTMLDivElement>(null)
  // The shift-range anchor is never rendered — a ref keeps `toggleSelect`
  // referentially stable.
  const anchorIdRef = useRef<string | null>(null)
  const isMobile = useIsMobile()
  // EXP-1048: the gestures are POINTER ones (hover checkbox, modifier clicks),
  // and a phone never opens this pane — so selection is the md+ arm only,
  // gated otherwise exactly like `IssueList` (a team plus moderation).
  const bulkEnabled = Boolean(bulkTeamId) && Boolean(canModerate) && !isMobile

  // The range / select-all universe: the rows actually ON SCREEN, in render
  // order — a folded group's rows are out (the big list's rule).
  const visibleFlatIssues = useMemo(
    () =>
      groups
        .filter(
          (group) => group.issues.length > 0 && !collapsed.has(group.status.id)
        )
        .flatMap((group) => group.issues),
    [groups, collapsed]
  )
  const visibleFlatIssuesRef = useRef(visibleFlatIssues)
  visibleFlatIssuesRef.current = visibleFlatIssues

  // Prune ids whose rows left the data set (deleted elsewhere, resynced) —
  // the IDE's `nav_selected.retain`. Folding a group keeps its rows selected.
  useEffect(() => {
    const present = new Set(
      groups.flatMap((group) => group.issues.map((issue) => issue.id))
    )
    setSelectedIds((prev) => {
      const next = new Set([...prev].filter((id) => present.has(id)))
      return next.size === prev.size ? prev : next
    })
  }, [groups, setSelectedIds])

  // An emptied selection (bar Clear, Esc, a list swap) drops the anchor too —
  // the next shift-click must not extend from a pre-clear row.
  useEffect(() => {
    if (selectedIds.size === 0) anchorIdRef.current = null
  }, [selectedIds])

  // A window narrowing past md takes the gestures with it, so the selection
  // goes too — a bar over rows that can no longer be selected is a dead end.
  useEffect(() => {
    if (bulkEnabled) return
    setSelectedIds((prev) => (prev.size > 0 ? new Set<string>() : prev))
  }, [bulkEnabled, setSelectedIds])

  const toggleSelect = useCallback(
    (issueId: string, shiftKey: boolean) => {
      const ids = visibleFlatIssuesRef.current.map((issue) => issue.id)
      const anchorId = anchorIdRef.current
      const anchorIndex = anchorId ? ids.indexOf(anchorId) : -1
      const targetIndex = ids.indexOf(issueId)
      if (shiftKey && anchorIndex !== -1 && targetIndex !== -1) {
        // Shift-click extends: ADD the contiguous visible slice between the
        // anchor and the target; the anchor stays put for further extensions.
        const [start, end] =
          anchorIndex < targetIndex
            ? [anchorIndex, targetIndex]
            : [targetIndex, anchorIndex]
        const range = ids.slice(start, end + 1)
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

  // Cmd/Ctrl+A selects every visible row, Escape clears — both overlay-scoped
  // exactly like the big list's: an Escape that dismisses a Radix menu must
  // not also wipe the selection, and select-all only fires with focus on the
  // body or inside this pane.
  useEffect(() => {
    if (!bulkEnabled) return
    const overlayOpen = () =>
      document.querySelector(
        `[data-state="open"][role="menu"], [data-state="open"][role="listbox"], [data-state="open"][role="dialog"], [data-state="open"][role="alertdialog"]`
      ) !== null
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === `a`) {
        if (overlayOpen()) return
        const active = document.activeElement
        if (
          active instanceof HTMLInputElement ||
          active instanceof HTMLTextAreaElement ||
          active instanceof HTMLSelectElement ||
          (active instanceof HTMLElement && active.isContentEditable)
        ) {
          return
        }
        if (
          active !== document.body &&
          active !== null &&
          !paneRef.current?.contains(active)
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
  }, [bulkEnabled, visibleFlatIssues, setSelectedIds])

  const anySelected = selectedIds.size > 0
  /** Where the row's content starts: its own padding plus the select cell. */
  const rowLead = PANE_ROW_PAD + (bulkEnabled ? SELECT_CELL : 0)

  return (
    <div
      ref={paneRef}
      className="flex-1 overflow-y-auto p-2"
      data-testid="board-issue-pane"
    >
      {visible.length === 0 && (
        <div className="px-3 py-2 text-xs text-muted-foreground">
          No issues in this board.
        </div>
      )}
      {visible.map((group) => {
        const open = !collapsed.has(group.status.id)
        // EXP-980: sub-issues sit under their parent here too (the groups
        // arrive nested from the list data hooks).
        const guides = group.depths ? treeGuides(group.depths) : null
        return (
          <div key={group.status.id} className="mb-2">
            <IssueGroupHeader
              status={group.status}
              count={group.issues.length}
              open={open}
              onToggle={() => toggle(group.status.id)}
              density="compact"
            />
            {open && (
              <div className="flex flex-col">
                {group.issues.map((issue, index) => {
                  const depth = group.depths?.[index] ?? 0
                  const selected = selectedIds.has(issue.id)
                  return (
                    // The select cell is the row's own left padding, so it
                    // sits ABOVE the link instead of inside it (an anchor
                    // holds no button) — the row keeps its whole width.
                    <div
                      key={issue.id}
                      className="group/row relative"
                      {...issueMenuProps(issue.id, from)}
                    >
                      <ListRow
                        asChild
                        interactive
                        // EXP-1048: a selected row wears the active fill, like
                        // the open one — both mean "this row is where you are"
                        // (the IDE's `active || selected`).
                        active={issue.id === activeIssueId || selected}
                        // EXP-862: the sidebar's compact density — 28px rows,
                        // titles truncating, no second line.
                        className="relative h-7 gap-2 px-2 py-0"
                        style={
                          rowLead !== PANE_ROW_PAD || depth > 0
                            ? { paddingLeft: rowLead + depth * TREE_INDENT }
                            : undefined
                        }
                      >
                        <Link
                          to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                          params={{
                            teamSlug,
                            boardSlug:
                              boardSlugById?.get(issue.boardId) ?? boardSlug,
                            issueIdentifier: issue.identifier,
                          }}
                          search={from ? { from } : {}}
                          // EXP-1048: Cmd/Ctrl-click toggles, Shift-click
                          // extends, a plain click navigates as before (the
                          // IDE's `nav_issue_row` click listener).
                          onClick={(event) => {
                            if (!bulkEnabled) return
                            if (event.metaKey || event.ctrlKey) {
                              event.preventDefault()
                              toggleSelect(issue.id, false)
                            } else if (event.shiftKey) {
                              event.preventDefault()
                              toggleSelect(issue.id, true)
                            }
                          }}
                        >
                          <TreeGuides guide={guides?.[index]} base={rowLead} />
                          <IssueStatusIcon
                            issue={issue}
                            className="!h-3.5 !w-3.5"
                          />
                          <span className="shrink-0 font-mono text-xs text-muted-foreground">
                            {issue.identifier}
                          </span>
                          <span className="min-w-0 flex-1 truncate text-sm">
                            {issue.title}
                          </span>
                        </Link>
                      </ListRow>
                      {bulkEnabled && (
                        <div
                          className="absolute inset-y-0 left-0 flex w-7 items-center pl-2"
                          // Suppress the browser's shift-click text selection
                          // so a range-select never highlights row text.
                          onMouseDown={(event) => {
                            if (event.shiftKey) event.preventDefault()
                          }}
                          onClick={(event) =>
                            toggleSelect(issue.id, event.shiftKey)
                          }
                        >
                          <Checkbox
                            checked={selected}
                            aria-label={`Select ${issue.identifier}`}
                            // Hover-revealed, then pinned while anything is
                            // selected (the IDE's `any_selected`).
                            className={`transition-opacity ${anySelected ? `opacity-100` : `opacity-0 group-hover/row:opacity-100`}`}
                          />
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
