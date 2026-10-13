import { useMemo } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { pullRequestCollection } from "@/lib/collections"
import type { PullRequestRow } from "@/lib/session-tree"
import {
  stackMergeConfirm,
  type StackConfirmMode,
  type StackMergeConfirm,
  type StackMergeNode,
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
