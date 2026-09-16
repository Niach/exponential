import { useEffect, useMemo, useRef, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import {
  ISSUE_SEARCH_DEFAULT_LIMIT,
  mergeIssueSearchHits,
  normalizeIssueSearchQuery,
  rankIssueSearch,
  type IssueSearchRow,
} from "@/lib/issue-search"

export type IssueSearchServerHit = Awaited<
  ReturnType<typeof trpc.issues.search.query>
>[number]

// EXP-892: the ONE web hook behind every issue list with a search box (the
// search sheet, the `#` autocomplete, the duplicate/relation picker, the
// composer's issue picker). Two layers, both from `lib/issue-search.ts`:
// the locally synced `rows` are ranked instantly on every keystroke, and
// ~250ms after typing stops the server's full-text pass (`issues.search`:
// title + description + comment bodies, stemmed) is spliced in behind them.
// Hits are keyed by the query they answered, so a stale response for an
// earlier keystroke never leaks into the current list; errors are swallowed
// — a search box must never block on the network.
export function useIssueSearchResults<T extends IssueSearchRow>({
  teamId,
  query,
  rows,
  limit = ISSUE_SEARCH_DEFAULT_LIMIT,
  exclude,
  resolveHit,
  server = true,
  emptyQuery = `recent`,
}: {
  /** The team `issues.search` runs against; undefined = local only. */
  teamId: string | undefined
  query: string
  /** The locally synced pool to rank. */
  rows: readonly T[]
  limit?: number
  exclude?: readonly string[]
  /** A server hit as a renderable row: the synced row by id, a stand-in
   *  built from the hit's fields, or null to drop it (a picker whose pool is
   *  a subset of the team never widens). */
  resolveHit: (hit: IssueSearchServerHit) => T | null
  /** false = never call the server (offline pickers, tests). */
  server?: boolean
  /** What an empty query lists: the newest rows, or nothing. */
  emptyQuery?: `recent` | `none`
}): { results: T[]; pending: boolean } {
  const normalized = normalizeIssueSearchQuery(query)
  const [hits, setHits] = useState<{
    query: string
    rows: IssueSearchServerHit[]
  }>({ query: ``, rows: [] })
  const wantServer = server && teamId !== undefined && normalized !== ``

  useEffect(() => {
    if (!wantServer) return
    let cancelled = false
    const timer = setTimeout(() => {
      trpc.issues.search
        .query({ teamId, query: normalized, limit: Math.min(limit, 50) })
        .then((rows) => {
          if (!cancelled) setHits({ query: normalized, rows })
        })
        .catch(() => {
          // Local ranking already rendered — never surface a network error.
        })
    }, 250)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [wantServer, teamId, normalized, limit])

  // `resolveHit` is a fresh closure per render; read it through a ref so the
  // merge memo keys on data alone.
  const resolveHitRef = useRef(resolveHit)
  resolveHitRef.current = resolveHit
  const excludeKey = exclude?.join(`,`) ?? ``

  const results = useMemo(() => {
    if (normalized === `` && emptyQuery === `none`) return [] as T[]
    const local = rankIssueSearch(rows, normalized, { limit, exclude })
    if (!wantServer || hits.query !== normalized) return local
    return mergeIssueSearchHits(local, hits.rows, {
      limit,
      exclude,
      resolve: (hit) => resolveHitRef.current(hit),
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, normalized, limit, excludeKey, hits, wantServer, emptyQuery])

  return { results, pending: wantServer && hits.query !== normalized }
}
