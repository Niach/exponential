import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { failingChecksNames, type PrCheckRowLike } from "@exp/domain-contract/pr-checks"
import { prChecksMenuEntry, type MenuEntry } from "@exp/ui"
import { pullRequestCheckCollection } from "@/lib/collections"
import { mergeMenuEntries, type MenuEntryId } from "@exp/domain-contract/merge-train"
import { prMergeState, type PullRequestRow } from "@/lib/session-tree"

// #EXP-1611 (pr-checks.json): one PR's synced `pull_request_checks` rows
// (its head commit), read by THE header status pill's tap and the `…`
// menu's Checks submenu. No client calls GitHub.

const toIso = (value: Date | string | null | undefined): string | null =>
  value == null ? null : typeof value === `string` ? value : value.toISOString()

const NONE: PrCheckRowLike[] = []

/** The PR's check rows as pr-checks.ts reads them (null id = none). */
export function usePullRequestChecks(pullRequestId: string | null | undefined): PrCheckRowLike[] {
  const { data } = useLiveQuery(
    (query) =>
      pullRequestId
        ? query
            .from({ checks: pullRequestCheckCollection })
            .where(({ checks }) => eq(checks.pullRequestId, pullRequestId))
        : undefined,
    [pullRequestId]
  )
  return useMemo(
    () =>
      data && data.length > 0
        ? data.map((row) => ({
            name: row.name,
            status: String(row.status),
            conclusion: row.conclusion == null ? null : String(row.conclusion),
            required: row.required,
            startedAt: toIso(row.startedAt),
            completedAt: toIso(row.completedAt),
            detailsUrl: row.detailsUrl ?? null,
          }))
        : NONE,
    [data]
  )
}

export { failingChecksNames }

/** The `…` menu's PR rows (#EXP-1710: merge-train.json `mergeMenuEntries`,
 *  amending session-thread.json `guideMenuEntries` ×4): the Checks submenu
 *  whenever the PR has check rows (any viewer), Merge now (owners), Hold |
 *  Resume auto-merge (members), Close (members); the callers own the rows. */
export function usePullRequestGuideEntries(
  pr: Pick<PullRequestRow, `id` | `state` | `mergeState`> | null,
  viewer: { member: boolean; owner: boolean },
  rows: Partial<Record<Exclude<MenuEntryId, `checks`>, MenuEntry | null>>
): MenuEntry[] {
  const checks = usePullRequestChecks(pr?.id)
  if (!pr) return []
  const mergeState = prMergeState(pr)
  return mergeMenuEntries({ state: pr.state, ...viewer, checks: checks.length, mergeState }).flatMap(
    (id): MenuEntry[] => {
      if (id === `checks`) {
        const entry = prChecksMenuEntry(checks, new Date())
        return entry ? [entry] : []
      }
      const row = rows[id]
      return row ? [row] : []
    }
  )
}
