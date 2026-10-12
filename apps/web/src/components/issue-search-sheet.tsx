import { useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLiveQuery, inArray } from "@tanstack/react-db"
import { Button, conceptIcon, EmptyState } from "@exp/ui"
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
import type { Board } from "@/db/schema"

const UiBackIcon = conceptIcon(`ui-back`)

const SearchGlyph = conceptIcon(`nav-search`)
const NoMatchGlyph = conceptIcon(`ui-empty-search`)

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
// so no row is ever the picked one. SLOP-28: the rows are THE search list's
// (status glyph, mono identifier, title), the same ones the `#` typeahead
// and every issue picker draw; the two-line board row is gone.
export function IssueSearchSheet({
  open,
  onOpenChange,
  teamId,
  teamSlug,
}: IssueSearchSheetProps) {
  const navigate = useNavigate()
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
      <EmptyState
        icon={SearchGlyph}
        title={ISSUE_SEARCH_EMPTY_HINT}
        description={ISSUE_SEARCH_EMPTY_DETAIL}
      />
    ) : (
      <EmptyState icon={NoMatchGlyph} title={ISSUE_SEARCH_NO_RESULTS} />
    )

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
      onPick={handlePick}
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
