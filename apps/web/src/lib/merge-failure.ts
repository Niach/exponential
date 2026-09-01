import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"

/**
 * EXP-533: what a refused merge means for the UI. `conflict` is the ONE gate
 * on the "Fix conflicts" recovery run: that run rebases, resolves and merges,
 * which only helps when the two trees actually disagree. A stale base, branch
 * protection, an unreachable server or a squash policy all refuse the merge
 * too, and offering the button there sends the agent in circles.
 */
export interface MergeFailure {
  message: string
  conflict: boolean
}

export function mergeFailure(error: unknown, fallback: string): MergeFailure {
  return { message: trpcErrorMessage(error, fallback), conflict: isConflict(error) }
}

function isConflict(error: unknown): boolean {
  // A REAL content conflict is the server's CONFLICT (409); every other
  // refusal (stale base, branch protection, policy) is PRECONDITION_FAILED.
  return trpcErrorCode(error) === `CONFLICT`
}
