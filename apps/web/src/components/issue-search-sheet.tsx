import { useState, useMemo } from "react"
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
  Input,
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
  useIsMobile,
  conceptIcon,
} from "@exp/ui"
import { issueCollection } from "@/lib/collections"
import { useIssueSearchResults } from "@/hooks/use-issue-search-results"
import type { IssueSearchRow } from "@/lib/issue-search"
import { useTeamBoards } from "@/hooks/use-team-data"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { BoardGlyph } from "@/components/board-glyph"
import { Search } from "lucide-react"
import type { Board } from "@/db/schema"

const UiBackIcon = conceptIcon(`ui-back`)

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

// One search experience, two presentations: a full-screen bottom sheet on
// mobile (reached from the topbar) and a centered cmdk dialog on desktop
// (reached from the sidebar or Cmd/Ctrl+F). The search logic is the shared
// engine (EXP-892, `useIssueSearchResults`: instant local ranking over the
// synced rows, the server's full-text pass spliced in behind); the desktop
// container is a `Command` so keyboard users get the shared list contract
// (top row selected, ↑/↓, Enter, hover moves the selection), while mobile
// stays touch-only.
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

  const emptyState =
    query.trim() === `` ? (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
        <Search className="size-8 mb-3 opacity-50" />
        <p className="text-sm">Type to search issues</p>
      </div>
    ) : (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
        <p className="text-sm">No issues match "{query}"</p>
      </div>
    )

  const resultRow = (issue: SearchResult) => {
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

  if (isMobile) {
    return (
      <Sheet open={open} onOpenChange={handleOpenChange}>
        {/* Page-like, not a sheet: it covers the whole screen, so it takes
            the New-issue page's chrome instead — no grabber, no radius, a
            leading back arrow where a sheet would have nothing (EXP-687). */}
        <SheetContent
          side="bottom"
          showGrabber={false}
          className="top-0 flex h-[100dvh] max-h-none flex-col gap-0 rounded-none p-0"
        >
          <SheetTitle className="sr-only">Search issues</SheetTitle>
          <div className="flex items-center gap-2 border-b border-border/50 px-3 py-3">
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
            <Search className="size-4 shrink-0 text-muted-foreground" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search issues..."
              autoFocus
              className="h-9 border-none text-base shadow-none focus-visible:ring-0 md:text-sm"
            />
          </div>
          <div className="flex-1 overflow-y-auto">
            {results.length === 0 && emptyState}
            {results.map((issue) => (
              <Button
                key={issue.id}
                type="button"
                variant="ghost"
                onClick={() => handlePick(issue)}
                className="flex h-auto w-full items-center justify-start gap-3 rounded-none px-4 py-3 text-left font-normal hover:bg-accent active:bg-accent/70 border-b border-border/30"
              >
                {resultRow(issue)}
              </Button>
            ))}
          </div>
        </SheetContent>
      </Sheet>
    )
  }

  // Desktop: cmdk owns the keyboard model (ArrowUp/Down move the highlighted
  // row, Enter opens it, the first result is pre-selected as results arrive).
  // Its internal filtering is off — `results` is already the local+server
  // merge — so items render exactly as computed.
  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="p-0 sm:p-0 gap-0 flex flex-col overflow-hidden sm:top-[15%] sm:max-h-[60vh] sm:translate-y-0 sm:max-w-lg"
      >
        <DialogTitle className="sr-only">Search issues</DialogTitle>
        <Command
          shouldFilter={false}
          className="min-h-0 bg-transparent **:data-[slot=command-input-wrapper]:h-14 **:data-[slot=command-input-wrapper]:border-border/50"
        >
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder="Search issues..."
            autoFocus
            className="text-base md:text-sm"
          />
          <CommandList className="max-h-none flex-1 overflow-y-auto">
            <CommandEmpty className="p-0">{emptyState}</CommandEmpty>
            {results.map((issue) => (
              <CommandItem
                key={issue.id}
                value={issue.id}
                onSelect={() => handlePick(issue)}
                className="gap-3 rounded-none px-4 py-3 cursor-pointer border-b border-border/30"
              >
                {resultRow(issue)}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  )
}
