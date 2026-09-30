import { TRPCError } from "@trpc/server"
import {
  GitHubMergeError,
  STACKED_PR_REFUSAL,
  UNMERGEABLE_405,
  type UnmergeableDiagnosis,
} from "@/lib/integrations/github-pr"

/**
 * The ONE mapping from a refused GitHub merge onto a tRPC error code, shared by
 * `issues.mergePr` and `repositories.mergePull`.
 *
 * EXP-533: the code IS the conflict signal every client gates its
 * "Fix conflicts" recovery run on. `CONFLICT` (HTTP 409) means the two trees
 * actually disagree and a rebase-and-resolve run can fix it; every other
 * refusal (stale base, branch protection, squash disallowed, a head that moved
 * under us) is `PRECONDITION_FAILED` (HTTP 412) and offering that button would
 * send the agent in circles.
 */

// GitHub's 405 "unmergeable" wordings live beside the smart merge
// (`UNMERGEABLE_405`, github-pr.ts): FEED-64 made the merge path verify a
// merged PR's own 405 before it reaches this mapping.
export function isNotMergeable(err: unknown): boolean {
  return (
    err instanceof GitHubMergeError &&
    err.status === 405 &&
    UNMERGEABLE_405.test(err.message)
  )
}

/**
 * FEED-43: GitHub's refusal to serve a STACK MEMBER through the legacy
 * endpoints — "Merging stacked PRs via this endpoint is not supported. Use the
 * asynchronous merge endpoint instead." (405 on merge, 422 on the base PATCH).
 * It is a routing signal, never a user-facing failure: the merge path retries
 * through merge-async, the retarget path says the stack owns the base.
 */
export function isStackedPrRefusal(err: unknown): boolean {
  return (
    err instanceof GitHubMergeError &&
    (err.status === 405 || err.status === 422) &&
    STACKED_PR_REFUSAL.test(err.message)
  )
}

export function prMergeFailureError(
  err: GitHubMergeError,
  diagnosis: UnmergeableDiagnosis | null
): TRPCError {
  if (err.status === 405) {
    if (isNotMergeable(err)) {
      // A diagnosis that failed to run (null) keeps today's behaviour: offer
      // the recovery run rather than hide it on an unknown state.
      const conflict = diagnosis?.conflict ?? true
      return new TRPCError({
        code: conflict ? `CONFLICT` : `PRECONDITION_FAILED`,
        message: diagnosis?.message ?? err.message,
      })
    }
    // Policy refusals (squash merges disallowed, branch protection): GitHub's
    // message is shown verbatim and no recovery run helps.
    return new TRPCError({ code: `PRECONDITION_FAILED`, message: err.message })
  }
  if (err.status === 409) {
    // GitHub's 409 is "head branch changed", not a content conflict.
    return new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Head branch changed on GitHub. Refresh and try again.`,
    })
  }
  if (err.status === 404) {
    return new TRPCError({
      code: `NOT_FOUND`,
      message: `Pull request not found on GitHub`,
    })
  }
  // FEED-64: the status is the diagnosis a bare "Server Error" lacks — and
  // by the time a 5xx reaches this mapping the PR has been re-read and is
  // NOT merged, so the caller may retry.
  return new TRPCError({
    code: `INTERNAL_SERVER_ERROR`,
    message: `GitHub merge failed (HTTP ${err.status}): ${err.message}`,
  })
}
