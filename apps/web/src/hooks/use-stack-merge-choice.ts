import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  pullRequestCollection,
  pullRequestIssueCollection,
} from "@/lib/collections"
import type { PullRequestRow } from "@/lib/session-tree"
import {
  stackMergeConfirm,
  stackView,
  type StackConfirmMode,
  type StackMergeConfirm,
  type StackMergeNode,
  type StackView,
} from "@/lib/pr-stack"

// EXP-1248 / EXP-1293: whether merging a pull request is a STACK merge (the
// ONE confirm, `stackMergeConfirm`), off the synced `pull_requests` rows of
// its repository: a PR's `head` is its branch, its `base` the branch it is
// stacked on. `input.issueId` of the confirm carries the PULL REQUEST id the
// merge lands through (`pullRequests.merge({pullRequestId, stack: true})`).

type StackPr = Pick<PullRequestRow, `id` | `number` | `head` | `base` | `state` | `url`>

function node(pr: StackPr): StackMergeNode {
  return {
    id: pr.id,
    identifier: `#${pr.number}`,
    branch: pr.head,
    prBaseBranch: pr.base,
    prState: pr.state,
    prUrl: pr.url,
  }
}

/** Pure: the stack confirm of `pr` among its repository's PRs (null = a plain
 *  merge). */
export function pullRequestStackConfirm(
  pr: StackPr,
  repositoryPrs: readonly StackPr[],
  mode: StackConfirmMode = `stack`
): StackMergeConfirm | null {
  return stackMergeConfirm(node(pr), repositoryPrs.map(node), mode)
}

export function usePullRequestStackConfirm(
  pr: Pick<PullRequestRow, `id` | `number` | `head` | `base` | `state` | `url` | `repositoryId`> | null,
  mode: StackConfirmMode = `stack`
): { ready: boolean; confirm: StackMergeConfirm | null } {
  const active = pr !== null && pr.state === `open`
  const repositoryId = pr?.repositoryId
  const { data, isReady } = useLiveQuery(
    (query) =>
      active && repositoryId
        ? query
            .from({ prs: pullRequestCollection })
            .where(({ prs }) => and(eq(prs.repositoryId, repositoryId), eq(prs.state, `open`)))
        : undefined,
    [active, repositoryId]
  )
  return useMemo(() => {
    if (!pr || !active) return { ready: true, confirm: null }
    if (!isReady) return { ready: false, confirm: null }
    return {
      ready: true,
      confirm: pullRequestStackConfirm(pr, (data ?? []) as PullRequestRow[], mode),
    }
  }, [pr, active, isReady, data, mode])
}

type StackViewPr = StackPr & Pick<PullRequestRow, `title`>

/** Pure: the stack rail of `pr` among its repository's PRs (null = not in a
 *  linear open stack). A row is named like the desktop's Stack card: the
 *  PR's lowest linked issue identifier + that issue's title, else `#<n>` +
 *  the PR's title. `issueId` of a row carries the PULL REQUEST id. */
export function pullRequestStackView(
  pr: StackViewPr,
  repositoryPrs: readonly StackViewPr[],
  issuesOf: (pullRequestId: string) => readonly { identifier: string; title: string }[]
): StackView | null {
  const viewNode = (row: StackViewPr) => {
    const named = [...issuesOf(row.id)].sort((a, b) =>
      a.identifier < b.identifier ? -1 : a.identifier > b.identifier ? 1 : 0
    )[0]
    return {
      ...node(row),
      identifier: named?.identifier ?? `#${row.number}`,
      title: named?.title ?? row.title ?? row.head,
      prNumber: row.number,
    }
  }
  return stackView(viewNode(pr), repositoryPrs.map(viewNode))
}

/** EXP-1306: the thread Guide's Stack card ×4 (session-thread.json): the
 *  Guide PR's stack rail off its repository's open PRs and their issues. */
export function usePullRequestStackView(
  pr: (StackViewPr & Pick<PullRequestRow, `repositoryId`>) | null
): StackView | null {
  const active = pr !== null && pr.state === `open`
  const repositoryId = pr?.repositoryId
  const { data: prs } = useLiveQuery(
    (query) =>
      active && repositoryId
        ? query
            .from({ prs: pullRequestCollection })
            .where(({ prs }) => and(eq(prs.repositoryId, repositoryId), eq(prs.state, `open`)))
        : undefined,
    [active, repositoryId]
  )
  const prIds = ((prs ?? []) as PullRequestRow[]).map((row) => row.id).sort()
  const prKey = prIds.join(`,`)
  const { data: named } = useLiveQuery(
    (query) =>
      prIds.length > 1
        ? query
            .from({ links: pullRequestIssueCollection })
            .where(({ links }) => inArray(links.pullRequestId, prIds))
            .innerJoin({ issues: issueCollection }, ({ links, issues }) =>
              eq(links.issueId, issues.id)
            )
            .select(({ links, issues }) => ({
              pullRequestId: links.pullRequestId,
              identifier: issues.identifier,
              title: issues.title,
            }))
        : undefined,
    [prKey]
  )
  return useMemo(() => {
    if (!pr || !active) return null
    const byPr = new Map<string, { identifier: string; title: string }[]>()
    for (const row of named ?? []) {
      const list = byPr.get(row.pullRequestId) ?? []
      list.push({ identifier: row.identifier, title: row.title })
      byPr.set(row.pullRequestId, list)
    }
    return pullRequestStackView(
      pr,
      (prs ?? []) as PullRequestRow[],
      (id) => byPr.get(id) ?? []
    )
  }, [pr, active, prs, named])
}
