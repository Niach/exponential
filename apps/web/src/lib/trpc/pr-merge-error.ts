import { TRPCError } from "@trpc/server"
import {
  GitHubMergeError,
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

/**
 * GitHub's 405 "unmergeable" refusal — the only 405 worth a base diagnosis.
 *
 * GitHub has shipped two wordings for the same state: the classic
 * `Pull Request is not mergeable` and, since 2026, the more specific
 * `Pull Request has merge conflicts` (EXP-737: that one slipped through as a
 * verbatim 412 policy refusal, so no client offered "Fix conflicts" on a real
 * conflict). Both mean the trees disagree; policy refusals ("Squash merges are
 * not allowed…", required reviews/checks) and the transient "Base branch was
 * modified" use neither phrase.
 */
const UNMERGEABLE_405 = /not mergeable|merge conflicts?/i

export function isNotMergeable(err: unknown): boolean {
  return (
    err instanceof GitHubMergeError &&
    err.status === 405 &&
    UNMERGEABLE_405.test(err.message)
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
  return new TRPCError({
    code: `INTERNAL_SERVER_ERROR`,
    message: `GitHub merge failed: ${err.message}`,
  })
}
