import { useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLiveQuery, inArray } from "@tanstack/react-db"
import { Button, conceptIcon, EmptyState } from "@exp/ui"
import { issueCollection, userCollection } from "@/lib/collections"
import { IssuePickerHost } from "@/components/issue-picker-host"
import {
  ISSUE_SEARCH_DEFAULT_LIMIT,
  ISSUE_SEARCH_EMPTY_DETAIL,
  ISSUE_SEARCH_EMPTY_HINT,
  ISSUE_SEARCH_NO_RESULTS,
  ISSUE_SEARCH_PLACEHOLDER,
  type IssueSearchRow,
} from "@/lib/issue-search"
import { useBoardsForTeams } from "@/hooks/use-team-data"
import { useAllAgentRows } from "@/hooks/use-agent-row"
import { useGlobalChatSearch } from "@/hooks/use-global-chat-search"
import { SEARCH_LIST_CHATS } from "@exp/domain-contract/search-list"
import type { Board, User } from "@/db/schema"
import { useStatusesForTeams } from "@/hooks/use-team-statuses"

const UiBackIcon = conceptIcon(`ui-back`)

const SearchGlyph = conceptIcon(`nav-search`)
const NoMatchGlyph = conceptIcon(`ui-empty-search`)

interface IssueSearchSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The teams searched: #EXP-2027 every member team on md+ (no active
   *  team), the route's team on a phone. The first leads the chats. */
  teams: readonly { id: string; slug: string }[]
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
  creatorId?: string | null
  creatorAgentId?: string | null
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
  teams,
}: IssueSearchSheetProps) {
  const navigate = useNavigate()
  const teamIds = useMemo(() => teams.map((team) => team.id), [teams])
  const slugOf = useMemo(() => new Map(teams.map((team) => [team.id, team.slug])), [teams])
  // #EDE-99 (search-list.json CHATS): the Chats section after the issues.
  const [query, setQuery] = useState(``)
  const chats = useGlobalChatSearch({ teams, query, open })
  const { boards } = useBoardsForTeams(teamIds)
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

  // #NEW-21: each row carries its creator's name + email and its agent's
  // name, so `by:lukas` and a bare `lukas` reach who filed it. #NEW-132:
  // and its team status NAME, so `status:review` reaches a custom status.
  const { data: users } = useLiveQuery((q) => (open ? q.from({ users: userCollection }) : undefined), [open])
  const userMap = useMemo(() => new Map(((users ?? []) as User[]).map((user) => [user.id, user])), [users])
  const agents = useAllAgentRows()
  const statusesOf = useStatusesForTeams(teamIds)
  const rows = useMemo(() => {
    const agentNames = new Map(agents.map((agent) => [agent.id, agent.name]))
    return ((issues ?? NO_ROWS) as SearchResult[]).map((issue) => {
      const creator = issue.creatorId ? userMap.get(issue.creatorId) : undefined
      return {
        ...issue,
        creatorName: creator?.name ?? null,
        creatorEmail: creator?.email ?? null,
        creatorAgentName: issue.creatorAgentId
          ? (agentNames.get(issue.creatorAgentId) ?? null)
          : null,
        statusName: statusesOf(boardMap.get(issue.boardId)?.teamId).resolve(issue).name,
      }
    })
  }, [issues, userMap, agents, statusesOf, boardMap])
  const localById = useMemo(
    () => new Map<string, SearchResult>(rows.map((i) => [i.id, i])),
    [rows]
  )

  const handlePick = (issue: SearchResult) => {
    const board = boardMap.get(issue.boardId)
    const teamSlug = board ? slugOf.get(board.teamId) : undefined
    if (!board || !teamSlug) return
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
      <EmptyState icon={NoMatchGlyph} title={chats ? SEARCH_LIST_CHATS.copy.noResults : ISSUE_SEARCH_NO_RESULTS} />
    )

  return (
    <IssuePickerHost
      shell="page"
      mode="single"
      open={open}
      onOpenChange={onOpenChange}
      title="Search issues"
      teamId={teamIds}
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
      searchPlaceholder={chats ? SEARCH_LIST_CHATS.copy.placeholder : ISSUE_SEARCH_PLACEHOLDER}
      chats={chats}
      onQueryInput={setQuery}
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
