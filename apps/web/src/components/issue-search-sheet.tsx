import { useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLiveQuery, inArray } from "@tanstack/react-db"
import { Button, useIsMobile, conceptIcon, BoardGlyph } from "@exp/ui"
import { issueCollection } from "@/lib/collections"
import { IssuePickerHost } from "@/components/issue-picker-host"
import {
  ISSUE_SEARCH_DEFAULT_LIMIT,
  ISSUE_SEARCH_EMPTY_DETAIL,
  ISSUE_SEARCH_EMPTY_HINT,
  ISSUE_SEARCH_NO_RESULTS,
  ISSUE_SEARCH_PLACEHOLDER,
  type IssueSearchRow,
} from "@/lib/issue-search"
import { useTeamBoards } from "@/hooks/use-team-data"
import {
  IssueStatusIcon,
} from "@/components/issue-properties/status-dropdown"
import type { Board } from "@/db/schema"

const UiBackIcon = conceptIcon(`ui-back`)

// EXP-971: the result rows are ONE flat list on both shells — full width,
// a hairline between rows, no inset card — and the row under the keyboard
// cursor or the pointer keeps the primitive's highlight, so the phone sheet
// and the desktop dialog read as the same list.
const FLAT_ROWS = `**:data-[slot=command-group]:p-0 **:data-[slot=command-item]:rounded-none **:data-[slot=command-item]:border-b **:data-[slot=command-item]:border-border/30 **:data-[slot=command-item]:px-4 **:data-[slot=command-item]:py-3`
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

// EXP-922: the ONE limit every search surface uses (web, desktop, iOS,
// Android) — the same query returns the same rows on every client; an empty
// query lists nothing (the hint shows instead).
const SEARCH_ENGINE = {
  limit: ISSUE_SEARCH_DEFAULT_LIMIT,
  emptyQuery: `none`,
} as const

// One engine, one body, two shells — THE issue picker host
// (`issue-picker-host.tsx`) in its `page` shell: a centred dialog on a
// pointer device (the sidebar, Cmd/Ctrl+F), a full-screen page on a phone
// with the back arrow inside the field's row (EXP-971). The engine lists
// nothing on an empty query (the hint shows instead), and picking NAVIGATES,
// so no row is ever the picked one. This surface keeps its own two-line row
// (the board and the identifier under the title, EXP-922).
export function IssueSearchSheet({
  open,
  onOpenChange,
  teamId,
  teamSlug,
}: IssueSearchSheetProps) {
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

  const handlePick = (issue: SearchResult) => {
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
  }

  // EXP-922: the empty states read the same on all four clients — one copy
  // set in `lib/issue-search.ts`, drift-gated by issue-search-surfaces.test.ts.
  const emptyState = (query: string) =>
    query.trim() === `` ? (
      <div className="flex flex-col items-center justify-center p-12 text-center text-muted-foreground">
        <SearchGlyph className="size-8 mb-3 opacity-50" />
        <p className="text-sm">{ISSUE_SEARCH_EMPTY_HINT}</p>
        <p className="text-xs mt-1 opacity-70">{ISSUE_SEARCH_EMPTY_DETAIL}</p>
      </div>
    ) : (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
        <p className="text-sm">{ISSUE_SEARCH_NO_RESULTS}</p>
      </div>
    )

  const renderRow = (issue: SearchResult) => {
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

  return (
    <IssuePickerHost
      shell="page"
      mode="single"
      open={open}
      onOpenChange={onOpenChange}
      title="Search issues"
      teamId={teamId}
      rows={rows}
      {...SEARCH_ENGINE}
      // Prefer the local Electric row when the id is synced locally so rows
      // render identically; otherwise render from the server fields.
      resolveHit={(hit) =>
        localById.get(hit.id) ?? {
          ...hit,
          description: null,
          createdAt: null,
          updatedAt: null,
        }
      }
      searchPlaceholder={ISSUE_SEARCH_PLACEHOLDER}
      emptyText={emptyState}
      statusGlyphs={false}
      renderItem={renderRow}
      onPick={handlePick}
      // The rows wear FLAT_ROWS on both shells; the shell caps the height.
      className={
        isMobile
          ? FLAT_ROWS
          : `${FLAT_ROWS} **:data-[slot=command-input-wrapper]:h-14 **:data-[slot=command-input-wrapper]:border-border/50`
      }
      listClassName="max-h-none"
      backButton={(close) => (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label="Back"
          onClick={close}
          className="shrink-0 text-muted-foreground"
        >
          <UiBackIcon className="size-4" />
        </Button>
      )}
    />
  )
}
