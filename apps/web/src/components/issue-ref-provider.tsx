import { createContext, useContext, useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import type { IconName } from "@exp/icons"
import { issueCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { statusColorCssValue } from "@/components/issue-properties/status-dropdown"
import { rankIssueSearch } from "@/lib/issue-search"
import type { Issue } from "@/db/schema"
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

const IssueRefContext = createContext<IssueRefContextValue | null>(null)

export function useIssueRefs(): IssueRefContextValue | null {
  return useContext(IssueRefContext)
}

export function IssueRefProvider({
  teamId,
  teamSlug,
  children,
}: {
  teamId: string | undefined
  teamSlug: string
  children: React.ReactNode
}) {
  const navigate = useNavigate()
  const boards = useTeamBoards(teamId)
  const boardIds = useMemo(() => boards.map((p) => p.id), [boards])
  const boardSlugById = useMemo(
    () => new Map(boards.map((p) => [p.id, p.slug])),
    [boards]
  )

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
  const { resolve: resolveStatus } = useTeamStatusesContext()

  const refs = useMemo(() => {
    const list: ResolvedIssueRef[] = []
    for (const issue of (issues ?? []) as Issue[]) {
      const boardSlug = boardSlugById.get(issue.boardId)
      if (!boardSlug) continue
      const statusOption = resolveStatus(issue)
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
        boardSlug,
      })
    }
    // Most recently created first, so empty-query search surfaces fresh work
    // (the engine's own empty-query order).
    return rankIssueSearch(list, ``, { limit: Number.POSITIVE_INFINITY })
  }, [issues, boardSlugById, resolveStatus])

  const byIdentifier = useMemo(
    () => new Map(refs.map((ref) => [ref.identifier.toUpperCase(), ref])),
    [refs]
  )

  const byId = useMemo(
    () => new Map(refs.map((ref) => [ref.id, ref])),
    [refs]
  )

  const value = useMemo<IssueRefContextValue>(
    () => ({
      teamId,
      rows: refs,
      resolve: (identifier) =>
        byIdentifier.get(identifier.toUpperCase()) ?? null,
      resolveById: (id) => byId.get(id) ?? null,
      fromHit: (hit) => {
        const local = byId.get(hit.id)
        if (local) return local
        const boardSlug = boardSlugById.get(hit.boardId)
        if (!boardSlug) return null
        const statusOption = resolveStatus({
          status: hit.status as IssueStatus,
          statusId: hit.statusId,
        })
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
          boardSlug,
        }
      },
      search: (query, opts) =>
        rankIssueSearch(refs, query, {
          limit: opts?.limit ?? 8,
          exclude: opts?.excludeIssueIds,
        }),
      open: (identifier, options) => {
        const ref = byIdentifier.get(identifier.toUpperCase())
        if (!ref) return
        void navigate({
          to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
          params: {
            teamSlug,
            boardSlug: ref.boardSlug,
            issueIdentifier: ref.identifier,
          },
          search: options?.from ? { from: options.from } : {},
        })
      },
    }),
    [teamId, byIdentifier, byId, refs, boardSlugById, resolveStatus, navigate, teamSlug]
  )

  return (
    <IssueRefContext.Provider value={value}>
      {children}
    </IssueRefContext.Provider>
  )
}
