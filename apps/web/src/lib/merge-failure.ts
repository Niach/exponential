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
  const code = trpcErrorCode(error)
  if (code === `CONFLICT`) return true
  // TRANSITIONAL (EXP-533): remove once every server answers a real conflict with 409
  return (
    code === `PRECONDITION_FAILED` &&
    trpcErrorMessage(error, ``).includes(`has merge conflicts with`)
  )
}
