// EXP-1248: stacked pull requests, server side. `pr_open` without a `base`
// stacks a PR on the nearest open PR below its head (`inferBaseBranch`); a
// stack here is `base_pull_request_id`. EXP-1709: no PR joins a GitHub
// native stack any more and the merge train lands a stack itself
// (merge-train.json CHILDREN).
import {
  compareRefs,
  GitHubCompareError,
  listOpenPulls,
  type GitHubFetch,
} from "@/lib/integrations/github-pr"

export interface InferredBase {
  /** The branch to open the PR on. */
  base: string
  /** The open PR whose head that is; null = the default branch. */
  prNumber: number | null
}

/** Bounds on `inferBaseBranch`'s compares (one GitHub call each, run in
 *  parallel): the newest candidates win the cut. */
export const MAX_BASE_CANDIDATES = 20

/**
 * `pr_open` without a `base`: the NEAREST open PR whose head is an ancestor
 * of `head` (compare API: `candidate...head` is `ahead`, fewest commits
 * ahead wins, the lower PR number on a tie), else `defaultBranch`.
 * `candidates` defaults to the repo's open PRs (callers narrow them to the
 * team's own PR branches); the newest `MAX_BASE_CANDIDATES` are compared in
 * parallel. A branch in `stopBranches` (one the repo is developed on) is
 * never a base PR. A candidate whose branch is gone (404) or whose compare
 * hits a GitHub 5xx is skipped; any other GitHub error throws.
 */
export async function inferBaseBranch(opts: {
  repo: string
  token: string
  head: string
  defaultBranch: string
  candidates?: ReadonlyArray<{ number: number; branch: string }>
  stopBranches?: readonly string[]
  fetchImpl?: GitHubFetch
}): Promise<InferredBase> {
  const stop = new Set([opts.defaultBranch, ...(opts.stopBranches ?? [])])
  const all =
    opts.candidates ??
    (await listOpenPulls(opts.repo, opts.token, opts.fetchImpl)).map((pull) => ({
      number: pull.number,
      branch: pull.branch,
    }))
  const candidates = all
    .filter(
      (candidate) =>
        candidate.branch &&
        candidate.branch !== opts.head &&
        !stop.has(candidate.branch)
    )
    .sort((a, b) => b.number - a.number)
    .slice(0, MAX_BASE_CANDIDATES)
  const compared = await Promise.all(
    candidates.map(async (candidate) => {
      try {
        const comparison = await compareRefs({
          repo: opts.repo,
          base: candidate.branch,
          head: opts.head,
          token: opts.token,
          ...(opts.fetchImpl ? { fetchImpl: opts.fetchImpl } : {}),
        })
        return { candidate, comparison }
      } catch (e) {
        if (e instanceof GitHubCompareError && e.status >= 500) {
          return { candidate, comparison: null }
        }
        throw e
      }
    })
  )
  let best: { number: number; branch: string; aheadBy: number } | null = null
  for (const { candidate, comparison } of compared) {
    if (comparison?.status !== `ahead` || comparison.aheadBy <= 0) continue
    if (
      !best ||
      comparison.aheadBy < best.aheadBy ||
      (comparison.aheadBy === best.aheadBy && candidate.number < best.number)
    ) {
      best = { ...candidate, aheadBy: comparison.aheadBy }
    }
  }
  return best
    ? { base: best.branch, prNumber: best.number }
    : { base: opts.defaultBranch, prNumber: null }
}
