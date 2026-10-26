import { TRPCError } from "@trpc/server"
import feedbackStates from "@exp/domain-contract/fixtures/feedback-states.json"
import { isRequiredCheckRefusal } from "@/lib/integrations/github-checks"
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
 * Fix conflicts run is gone): every refusal is PRECONDITION_FAILED except
 * the check gate's, PRECONDITION_REQUIRED (HTTP 428), the one a person may
 * override with `force` (feedback-states.json errors.mergeRefusal).
 */

export type MergeRefusalKind = `checks_pending` | `checks_failed` | `conflict` | `other`

export const MERGE_REFUSAL_COPY = feedbackStates.errors.mergeRefusal

export class MergeRefusal extends TRPCError {
  readonly kind: MergeRefusalKind

  constructor(opts: {
    kind: MergeRefusalKind
    message: string
    code?: `PRECONDITION_FAILED` | `PRECONDITION_REQUIRED`
  }) {
    super({ code: opts.code ?? `PRECONDITION_FAILED`, message: opts.message })
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
    // Branch protection still waiting for a required check (EXP-1391: the
    // "Required status check "gate" is expected." refusal); our gate saw no
    // check yet. `force` cannot override GitHub's own rule.
    if (isRequiredCheckRefusal(err.message)) {
      return new MergeRefusal({ kind: `checks_pending`, message: err.message })
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

/** EXP-1378: the check gate's refusal (the code `force` overrides). */
export function checksRefusal(
  verdict: { state: `pending` | `red`; names: string[] },
  prNumber: number
): MergeRefusal {
  const names = verdict.names.map((name) => `"${name}"`).join(`, `)
  return new MergeRefusal({
    kind: verdict.state === `pending` ? `checks_pending` : `checks_failed`,
    code: `PRECONDITION_REQUIRED`,
    message:
      verdict.state === `pending`
        ? `Checks are still running on PR #${prNumber}: ${names}.`
        : `Checks failed on PR #${prNumber}: ${names}.`,
  })
}
