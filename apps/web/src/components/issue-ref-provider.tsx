import { createContext, useCallback, useContext, useMemo, useRef } from "react"
import { useNavigate } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import type { IconName } from "@exp/icons"
import { issueCollection } from "@/lib/collections"
import { useBoardsForTeams, useTeamBoards } from "@/hooks/use-team-data"
import { useStatusesForTeams, useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusResolvable, StatusRowOption } from "@/lib/team-statuses"
import { statusColorCssValue } from "@/components/issue-properties/status-dropdown"
import { rankIssueSearch } from "@/lib/issue-search"
import type { Board, Issue } from "@/db/schema"
import type { IssueStatus } from "@/lib/domain"

// Team-scoped issue-reference resolution, mounted once in the team
// layout. Powers the `#IDENTIFIER` pill rendering in the markdown editors, the
// #-autocomplete in the comment composer, and the mark-as-duplicate issue
// picker — resolution from the already-synced issues shape; searching runs
// the shared engine (EXP-892, `lib/issue-search.ts`) over `rows`, and the
// consumers that can wait add the server's full-text pass through
// `useIssueSearchResults`.
// Resolution is scoped to the current team's boards so a same-prefix
// identifier from another team never leaks in.

export interface ResolvedIssueRef {
  id: string
  identifier: string
  title: string
  // EXP-892: the search engine (`lib/issue-search.ts`) ranks descriptions
  // and breaks ties on recency, so the rows carry them.
  description: string | null
  createdAt: string | Date | null
  updatedAt: string | Date | null
  boardId: string
  status: IssueStatus
  // EXP-314: carried so the ref/autocomplete/duplicate-picker rows render the
  // team's own status glyph, not just the anchor's.
  statusId: string | null
  // EXP-423: the resolved glyph + a concrete CSS color, precomputed here so
  // the pill decoration (which has no React context) can render the status
  // icon straight from a style attribute.
  statusIcon: IconName
  statusColor: string
  boardSlug: string
}

export interface IssueRefContextValue {
  /** The team the refs resolve in (undefined until the layout knows it). */
  teamId: string | undefined
  /** #EXP-2079: set by a chat's own scope (`ChatTeamsScope`): its chat
   *  teams, so a nested scope of the same chat mounts nothing. */
  scopeKey?: string
  /** #EXP-2079: the chat teams of that scope, in order (absent = `teamId`
   *  alone); a team-less link of the chat resolves against the first. */
  teamIds?: readonly string[]
  /** Every visible issue of the team, newest created first — the pool the
   * search hook ranks (`useIssueSearchResults`). */
  rows: ResolvedIssueRef[]
  /** Resolve an identifier (case-insensitive) to a visible issue, or null. */
  resolve: (identifier: string) => ResolvedIssueRef | null
  /** Resolve a row UUID to a visible issue, or null — the relations card
   * reads ids off the issue_relations shape, never identifiers. */
  resolveById: (id: string) => ResolvedIssueRef | null
  /** EXP-892: a server full-text hit as a ref row — the synced row when the
   * id is local, else a stand-in from the hit's fields (its board must be
   * known, or null). */
  fromHit: (hit: {
    id: string
    identifier: string
    title: string
    boardId: string
    status: string
    statusId: string | null
  }) => ResolvedIssueRef | null
  /** Local ranking only (`lib/issue-search.ts`); empty query = most recent.
   * Async consumers use `useIssueSearchResults` for the server pass. */
  search: (
    query: string,
    opts?: { excludeIssueIds?: string[]; limit?: number }
  ) => ResolvedIssueRef[]
  /** Navigate to an issue's full-page detail route by identifier. `from` =
   * the list the detail keeps beside it (EXP-851, `lib/detail-origin.ts`). */
  open: (identifier: string, options?: { from?: string }) => void
}

export const IssueRefContext = createContext<IssueRefContextValue | null>(null)

export function useIssueRefs(): IssueRefContextValue | null {
  return useContext(IssueRefContext)
}

/** The context value over `boards` (one team's, or several teams' in the
 *  teams' order): an identifier found in several teams is the first team's
 *  (`teamRank`, lowest wins), every issue opens through its OWN team. */
function useIssueRefValue({
  teamId,
  boards,
  resolveStatus,
  teamSlugOf,
  teamRank,
}: {
  teamId: string | undefined
  boards: readonly Board[]
  resolveStatus: (issue: StatusResolvable, teamId: string) => StatusRowOption
  teamSlugOf: (teamId: string) => string | undefined
  teamRank: (teamId: string) => number
}): IssueRefContextValue {
  const navigate = useNavigate()
  const boardIds = useMemo(() => boards.map((p) => p.id), [boards])
  const boardById = useMemo(() => new Map(boards.map((p) => [p.id, p])), [boards])

  const { data: issues } = useLiveQuery(
    (query) =>
      boardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.boardId, boardIds))
        : undefined,
    [boardIds.join(`,`)]
  )

  // The team's status rows are already live-queried one level up (the layout
  // nests this provider inside TeamStatusesProvider), so a rename/recolor/
  // reorder rotates this context value and every consumer that reads a ref
  // during render repaints immediately — open tiptap editors included.
  //
  // That last part is worth spelling out, because the editors read
  // `getResolved` through a ref and their pill decorations only rebuild when
  // ProseMirror updates its view. The chain that closes the gap: an in-place
  // row update makes useLiveQuery hand back a fresh `data` array (its snapshot
  // is version-counted, not value-compared), which rotates `resolveStatus`,
  // then the `refs` memo and the context value below — and markdown-editor.tsx
  // watches that value to dispatch a no-op transaction, which re-runs the
  // decoration pass against the new glyph and tint.
  const refs = useMemo(() => {
    const list: ResolvedIssueRef[] = []
    for (const issue of (issues ?? []) as Issue[]) {
      const board = boardById.get(issue.boardId)
      if (!board) continue
      const statusOption = resolveStatus(issue, board.teamId)
      list.push({
        id: issue.id,
        identifier: issue.identifier,
        title: issue.title,
        description: issue.description ?? null,
        createdAt: issue.createdAt,
        updatedAt: issue.updatedAt,
        boardId: issue.boardId,
        status: issue.status,
        statusId: issue.statusId,
        statusIcon: statusOption.icon,
        statusColor: statusColorCssValue(statusOption),
        boardSlug: board.slug,
      })
    }
    // Most recently created first, so empty-query search surfaces fresh work
    // (the engine's own empty-query order).
    return rankIssueSearch(list, ``, { limit: Number.POSITIVE_INFINITY })
  }, [issues, boardById, resolveStatus])

  const byIdentifier = useMemo(() => {
    const map = new Map<string, ResolvedIssueRef>()
    const rankOf = (ref: ResolvedIssueRef) => teamRank(boardById.get(ref.boardId)?.teamId ?? ``)
    for (const ref of refs) {
      const key = ref.identifier.toUpperCase()
      const held = map.get(key)
      if (!held || rankOf(ref) < rankOf(held)) map.set(key, ref)
    }
    return map
  }, [refs, boardById, teamRank])

  const byId = useMemo(
    () => new Map(refs.map((ref) => [ref.id, ref])),
    [refs]
  )

  return useMemo<IssueRefContextValue>(
    () => ({
      teamId,
      rows: refs,
      resolve: (identifier) =>
        byIdentifier.get(identifier.toUpperCase()) ?? null,
      resolveById: (id) => byId.get(id) ?? null,
      fromHit: (hit) => {
        const local = byId.get(hit.id)
        if (local) return local
        const board = boardById.get(hit.boardId)
        if (!board) return null
        const statusOption = resolveStatus(
          { status: hit.status as IssueStatus, statusId: hit.statusId },
          board.teamId
        )
        return {
          id: hit.id,
          identifier: hit.identifier,
          title: hit.title,
          description: null,
          createdAt: null,
          updatedAt: null,
          boardId: hit.boardId,
          status: hit.status as IssueStatus,
          statusId: hit.statusId,
          statusIcon: statusOption.icon,
          statusColor: statusColorCssValue(statusOption),
          boardSlug: board.slug,
        }
      },
      search: (query, opts) =>
        rankIssueSearch(refs, query, {
          limit: opts?.limit ?? 8,
          exclude: opts?.excludeIssueIds,
        }),
      open: (identifier, options) => {
        const ref = byIdentifier.get(identifier.toUpperCase())
        const slug = ref ? teamSlugOf(boardById.get(ref.boardId)?.teamId ?? ``) : undefined
        if (!ref || !slug) return
        void navigate({
          to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
          params: {
            teamSlug: slug,
            boardSlug: ref.boardSlug,
            issueIdentifier: ref.identifier,
          },
          search: options?.from ? { from: options.from } : {},
        })
      },
    }),
    [teamId, byIdentifier, byId, refs, boardById, resolveStatus, navigate, teamSlugOf]
  )
}

const ONE_TEAM_RANK = () => 0

export function IssueRefProvider({
  teamId,
  teamSlug,
  children,
}: {
  teamId: string | undefined
  teamSlug: string
  children: React.ReactNode
}) {
  const boards = useTeamBoards(teamId)
  const { resolve } = useTeamStatusesContext()
  const teamSlugOf = useCallback(() => teamSlug, [teamSlug])
  const value = useIssueRefValue({ teamId, boards, resolveStatus: resolve, teamSlugOf, teamRank: ONE_TEAM_RANK })
  return (
    <IssueRefContext.Provider value={value}>
      {children}
    </IssueRefContext.Provider>
  )
}

/** #EXP-2079 (agent-settings.json `assistant.chatTeams` (1), (6), (7)): the
 *  refs of a chat that reads SEVERAL teams (the Assistant's conversation):
 *  the issues of `teams` in their order, each with its OWN team's status
 *  glyph, each opening through its own team. `teamId` = the first team (the
 *  server's full-text pass). */
export function ChatTeamsIssueRefProvider({
  teams,
  scopeKey,
  children,
}: {
  teams: readonly { id: string; slug: string }[]
  scopeKey: string
  children: React.ReactNode
}) {
  const key = teams.map((team) => `${team.id}:${team.slug}`).join(`,`)
  const teamIds = useMemo(() => teams.map((team) => team.id), [key])
  const { boards } = useBoardsForTeams(teamIds)
  const statusesOf = useStatusesForTeams(teamIds)
  const resolveStatus = useCallback(
    (issue: StatusResolvable, teamId: string) => statusesOf(teamId).resolve(issue),
    [statusesOf]
  )
  const teamSlugOf = useCallback((teamId: string) => teams.find((team) => team.id === teamId)?.slug, [key])
  const teamRank = useCallback(
    (teamId: string) => {
      const at = teamIds.indexOf(teamId)
      return at < 0 ? teamIds.length : at
    },
    [teamIds]
  )
  const refs = useIssueRefValue({ teamId: teamIds[0], boards, resolveStatus, teamSlugOf, teamRank })
  const value = useMemo(() => ({ ...refs, scopeKey, teamIds }), [refs, scopeKey, teamIds])
  return <IssueRefContext.Provider value={value}>{children}</IssueRefContext.Provider>
}

/** EXP-1347 (session-chat.json): chip navigation = the INNERMOST outlet.
 *  Inside a chat, a pill (#IDENT, an issue chip, a card chip) whose issue
 *  is one of `issueIds` (that chat's issue tabs, nested ones included)
 *  selects that tab (`onOpen`) and never leaves the chat; any other issue
 *  defers to the enclosing provider (the full issue page). */
export function IssueRefOutlet({
  issueIds,
  onOpen,
  children,
}: {
  issueIds: readonly string[]
  onOpen: (issueId: string) => void
  children: React.ReactNode
}) {
  const parent = useIssueRefs()
  // The callback rides a ref: a fresh `onOpen` per render must not rotate
  // the context value (the editors repaint their pills on every rotation).
  const onOpenRef = useRef(onOpen)
  onOpenRef.current = onOpen
  const key = [...issueIds].sort().join(`,`)
  const value = useMemo<IssueRefContextValue | null>(() => {
    if (!parent) return null
    const own = new Set(key ? key.split(`,`) : [])
    return {
      ...parent,
      open: (identifier, options) => {
        const ref = parent.resolve(identifier)
        if (ref && own.has(ref.id)) {
          onOpenRef.current(ref.id)
          return
        }
        parent.open(identifier, options)
      },
    }
  }, [parent, key])
  if (!value) return <>{children}</>
  return <IssueRefContext.Provider value={value}>{children}</IssueRefContext.Provider>
}
