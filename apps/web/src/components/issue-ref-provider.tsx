import { createContext, useContext, useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { issueCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import type { Issue } from "@/db/schema"
import type { IssueStatus } from "@/lib/domain"

// Team-scoped issue-reference resolution, mounted once in the team
// layout. Powers the `#IDENTIFIER` pill rendering in the markdown editors, the
// #-autocomplete in the comment composer, and the mark-as-duplicate issue
// picker — all from the already-synced issues shape (no server round-trips).
// Resolution is scoped to the current team's boards so a same-prefix
// identifier from another team never leaks in.

export interface ResolvedIssueRef {
  id: string
  identifier: string
  title: string
  status: IssueStatus
  boardSlug: string
}

export interface IssueRefContextValue {
  /** Resolve an identifier (case-insensitive) to a visible issue, or null. */
  resolve: (identifier: string) => ResolvedIssueRef | null
  /** Search visible issues by identifier/title; empty query = most recent. */
  search: (
    query: string,
    opts?: { excludeIssueIds?: string[]; limit?: number }
  ) => ResolvedIssueRef[]
  /** Navigate to an issue's full-page detail route by identifier. */
  open: (identifier: string) => void
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

  const refs = useMemo(() => {
    const list: ResolvedIssueRef[] = []
    for (const issue of (issues ?? []) as Issue[]) {
      const boardSlug = boardSlugById.get(issue.boardId)
      if (!boardSlug) continue
      list.push({
        id: issue.id,
        identifier: issue.identifier,
        title: issue.title,
        status: issue.status,
        boardSlug,
      })
    }
    // Most recently created first, so empty-query search surfaces fresh work.
    const createdAt = new Map(
      ((issues ?? []) as Issue[]).map((issue) => [
        issue.id,
        new Date(issue.createdAt).getTime(),
      ])
    )
    list.sort((a, b) => (createdAt.get(b.id) ?? 0) - (createdAt.get(a.id) ?? 0))
    return list
  }, [issues, boardSlugById])

  const byIdentifier = useMemo(
    () => new Map(refs.map((ref) => [ref.identifier.toUpperCase(), ref])),
    [refs]
  )

  const value = useMemo<IssueRefContextValue>(
    () => ({
      resolve: (identifier) =>
        byIdentifier.get(identifier.toUpperCase()) ?? null,
      search: (query, opts) => {
        const q = query.trim().toLowerCase()
        const exclude = new Set(opts?.excludeIssueIds ?? [])
        const limit = opts?.limit ?? 8
        const matches: ResolvedIssueRef[] = []
        for (const ref of refs) {
          if (exclude.has(ref.id)) continue
          if (
            q &&
            !ref.identifier.toLowerCase().includes(q) &&
            !ref.title.toLowerCase().includes(q)
          ) {
            continue
          }
          matches.push(ref)
          if (matches.length >= limit) break
        }
        return matches
      },
      open: (identifier) => {
        const ref = byIdentifier.get(identifier.toUpperCase())
        if (!ref) return
        void navigate({
          to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
          params: {
            teamSlug,
            boardSlug: ref.boardSlug,
            issueIdentifier: ref.identifier,
          },
        })
      },
    }),
    [byIdentifier, refs, navigate, teamSlug]
  )

  return (
    <IssueRefContext.Provider value={value}>
      {children}
    </IssueRefContext.Provider>
  )
}
