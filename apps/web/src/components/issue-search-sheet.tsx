import { useState, useMemo, useEffect, useRef } from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLiveQuery, inArray } from "@tanstack/react-db"
import {
  Sheet,
  SheetContent,
  SheetTitle,
  Dialog,
  DialogContent,
  DialogTitle,
  Button,
  ComboboxList,
  useIsMobile,
  conceptIcon,
  type PickerOption,
  BoardGlyph,
} from "@exp/ui"
import { issueCollection } from "@/lib/collections"
import { useIssueSearchResults } from "@/hooks/use-issue-search-results"
import type { IssueSearchRow } from "@/lib/issue-search"
import { useTeamBoards } from "@/hooks/use-team-data"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import type { Board } from "@/db/schema"

const UiBackIcon = conceptIcon(`ui-back`)
const SearchGlyph = conceptIcon(`nav-search`)

interface IssueSearchSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  teamSlug: string
}

// The minimal fields a result row needs to render + navigate. Local Electric
// `Issue` rows satisfy this structurally; server FTS hits (issues.search)
// provide exactly these fields (the engine's ranking fields stay null on a
// stand-in — it only ever sorts local rows).
interface SearchResult extends IssueSearchRow {
  id: string
  identifier: string
  title: string
  boardId: string
  status: string
  // EXP-314: server FTS hits carry it too, so status icons resolve to the
  // team's own rows for issues that aren't synced locally.
  statusId: string | null
}

const NO_ROWS: SearchResult[] = []

// One engine, one body, two shells.
//
// The engine is the shared one (EXP-892, `useIssueSearchResults`: instant
// local ranking over the synced rows, the server's full-text pass spliced in
// behind). The body is the shared picker (EXP-941/EXP-958, `ComboboxList`):
// the search field, the rows and the empty state all come from it, so this
// surface can no longer drift from every other searchable list — cmdk owns
// the keyboard model (top row selected as results arrive, ↑/↓, Enter opens,
// hover moves the selection) and its own filter stays OFF, since `results` is
// already the local+server merge. Picking NAVIGATES, so `value={null}`: no
// row is ever the picked one and none wears a check.
//
// Only the shell differs: a full-screen page-like sheet on mobile (reached
// from the topbar, back arrow above the field) and a centered dialog on
// desktop (reached from the sidebar or Cmd/Ctrl+F).
export function IssueSearchSheet({
  open,
  onOpenChange,
  teamId,
  teamSlug,
}: IssueSearchSheetProps) {
  const [query, setQuery] = useState(``)
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const boards = useTeamBoards(teamId)
  const boardIds = useMemo(() => boards.map((p: Board) => p.id), [boards])
  const boardMap = useMemo(
    () => new Map<string, Board>(boards.map((p: Board) => [p.id, p])),
    [boards]
  )

  const { data: issues } = useLiveQuery(
    (q) =>
      boardIds.length > 0 && open
        ? q
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.boardId, boardIds))
        : undefined,
    [boardIds.join(`,`), open]
  )

  const rows = (issues ?? NO_ROWS) as SearchResult[]
  const localById = useMemo(
    () => new Map<string, SearchResult>(rows.map((i) => [i.id, i])),
    [rows]
  )

  const { results } = useIssueSearchResults<SearchResult>({
    teamId,
    query,
    rows,
    limit: 30,
    // Prefer the local Electric row when the id is synced locally so rows
    // render identically; otherwise render from the server fields.
    resolveHit: (hit) =>
      localById.get(hit.id) ?? {
        ...hit,
        description: null,
        createdAt: null,
        updatedAt: null,
      },
    server: open,
    emptyQuery: `none`,
  })

  // The option carries the IDENTITY only; the row it stands for is looked up
  // here (two issues can share a title).
  const options = useMemo<PickerOption[]>(
    () =>
      results.map((issue) => ({
        value: issue.id,
        label: issue.title,
        keywords: [issue.identifier, issue.title],
      })),
    [results]
  )
  const resultById = useMemo(
    () => new Map<string, SearchResult>(results.map((i) => [i.id, i])),
    [results]
  )

  const handleOpenChange = (o: boolean) => {
    onOpenChange(o)
    if (!o) setQuery(``)
  }

  const handlePick = (issue: SearchResult) => {
    const board = boardMap.get(issue.boardId)
    if (!board) return
    onOpenChange(false)
    setQuery(``)
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug: board.slug,
        issueIdentifier: issue.identifier,
      },
    })
  }

  // cmdk's input takes no `autoFocus`, and both shells are Radix dialogs whose
  // own open-focus lands on the first tabbable child — the back arrow on
  // mobile. So the field is focused explicitly once the shell is mounted.
  const shellRef = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    if (!open) return
    const frame = requestAnimationFrame(() => {
      shellRef.current
        ?.querySelector<HTMLInputElement>(`[data-slot=command-input]`)
        ?.focus()
    })
    return () => cancelAnimationFrame(frame)
  }, [open])

  const emptyState =
    query.trim() === `` ? (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
        <SearchGlyph className="size-8 mb-3 opacity-50" />
        <p className="text-sm">Type to search issues</p>
      </div>
    ) : (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
        <p className="text-sm">No issues match "{query}"</p>
      </div>
    )

  const renderOption = (option: PickerOption) => {
    const issue = resultById.get(option.value)
    if (!issue) return null
    const board = boardMap.get(issue.boardId)
    return (
      <>
        <IssueStatusIcon issue={issue} className="size-4 shrink-0" />
        <div className="flex flex-col flex-1 min-w-0">
          <span className="text-sm truncate">{issue.title}</span>
          {board && (
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <BoardGlyph board={board} className="size-3" />
              <span className="truncate">
                {board.name} · {issue.identifier}
              </span>
            </span>
          )}
        </div>
      </>
    )
  }

  // The list is the whole body; the shell caps its height, so the primitive's
  // own 18.75rem cap comes off.
  const list = (className?: string) => (
    <ComboboxList
      options={options}
      value={null}
      onChange={(id) => {
        const issue = id === null ? undefined : resultById.get(id)
        if (issue) handlePick(issue)
      }}
      shouldFilter={false}
      query={query}
      onQueryChange={setQuery}
      placeholder="Search issues..."
      emptyText={emptyState}
      renderOption={renderOption}
      className={className}
      listClassName="max-h-none"
    />
  )

  if (isMobile) {
    return (
      <Sheet open={open} onOpenChange={handleOpenChange}>
        {/* Page-like, not a sheet: it covers the whole screen, so it takes
            the New-issue page's chrome instead — no grabber, no radius, a
            leading back arrow where a sheet would have nothing (EXP-687).
            The arrow is its own slim row above the field, which the shared
            body owns. */}
        <SheetContent
          ref={shellRef}
          side="bottom"
          showGrabber={false}
          className="top-0 flex h-[100dvh] max-h-none flex-col gap-0 rounded-none p-0"
        >
          <SheetTitle className="sr-only">Search issues</SheetTitle>
          <div className="flex items-center border-b border-border/50 px-3 py-2">
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label="Back"
              onClick={() => handleOpenChange(false)}
              className="shrink-0 text-muted-foreground"
            >
              <UiBackIcon className="size-4" />
            </Button>
          </div>
          {list(
            `**:data-[slot=command-input]:text-base **:data-[slot=command-input-wrapper]:h-12 **:data-[slot=command-input-wrapper]:border-border/50`
          )}
        </SheetContent>
      </Sheet>
    )
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        ref={shellRef}
        showCloseButton={false}
        className="p-0 sm:p-0 gap-0 flex flex-col overflow-hidden sm:top-[15%] sm:max-h-[60vh] sm:translate-y-0 sm:max-w-lg"
      >
        <DialogTitle className="sr-only">Search issues</DialogTitle>
        {list(
          `**:data-[slot=command-input-wrapper]:h-14 **:data-[slot=command-input-wrapper]:border-border/50`
        )}
      </DialogContent>
    </Dialog>
  )
}
