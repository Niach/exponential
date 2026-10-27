import { TRPCError } from "@trpc/server"
import feedbackStates from "@exp/domain-contract/fixtures/feedback-states.json"
import {
  GitHubMergeError,
  UNMERGEABLE_405,
  type UnmergeableDiagnosis,
} from "@/lib/integrations/github-pr"

/**
 * The ONE mapping from a refused merge onto a tRPC error, shared by every
 * merge path (`mergePullRequestOrThrow`, lib/pull-requests.ts).
 *
 * EXP-1391: a refusal is a `MergeRefusal` carrying its KIND, which picks the
 * steps the thread's root session reads when the refusal is posted into it
 * (`lib/merge-refusal.ts`). No client classifies a conflict any more (the
 * Fix conflicts run is gone): every refusal is PRECONDITION_FAILED and
 * every client offers Retry (feedback-states.json errors.mergeRefusal).
 */

export type MergeRefusalKind = `checks` | `conflict` | `other`

export const MERGE_REFUSAL_COPY = feedbackStates.errors.mergeRefusal

export class MergeRefusal extends TRPCError {
  readonly kind: MergeRefusalKind
  /** The refused PR (stamped by the merge path). */
  prNumber: number | null = null

  constructor(opts: { kind: MergeRefusalKind; message: string }) {
    super({ code: `PRECONDITION_FAILED`, message: opts.message })
    this.kind = opts.kind
  }
}

export function isMergeRefusal(err: unknown): err is MergeRefusal {
  return err instanceof MergeRefusal
}

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

export function prMergeFailureError(
  err: GitHubMergeError,
  diagnosis: UnmergeableDiagnosis | null
): TRPCError {
  if (err.status === 405) {
    if (isNotMergeable(err)) {
      // `conflict` only on a DIAGNOSED content conflict: an undiagnosed 405
      // (stale base, a vanished branch) is GitHub's message, kind other.
      return new MergeRefusal({
        kind: diagnosis?.conflict === true ? `conflict` : `other`,
        message: diagnosis?.message || err.message,
      })
    }
    // Branch protection waiting on a required check, or one that failed
    // (EXP-1391: GitHub's "Required status check "gate" is expected.").
    if (isRequiredCheckRefusal(err.message)) {
      return new MergeRefusal({ kind: `checks`, message: err.message })
    }
    // Policy refusals (squash merges disallowed, branch protection): GitHub's
    // message verbatim.
    return new MergeRefusal({ kind: `other`, message: err.message })
  }
  if (err.status === 409) {
    // GitHub's 409 is "head branch changed", not a content conflict.
    return new MergeRefusal({
      kind: `other`,
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

/** GitHub's branch protection refusing on a required status check. */
export function isRequiredCheckRefusal(message: string): boolean {
  return /required status checks?\b/i.test(message)
}
