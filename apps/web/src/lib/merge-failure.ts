import feedbackStates from "@exp/domain-contract/fixtures/feedback-states.json"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"

const MERGE_REFUSAL = feedbackStates.errors.mergeRefusal

/** The check gate's override label (`Merge anyway`). */
export const MERGE_ANYWAY_LABEL = MERGE_REFUSAL.force

/**
 * EXP-1391 + EXP-1378: what a refused merge means for the UI. `checks` =
 * the check gate refused (PRECONDITION_REQUIRED: required checks running or
 * red): its toast offers `Merge anyway` (`force: true`) instead of Retry.
 * Nothing reads a conflict any more: the refusal went into the thread and
 * its session takes it over (the message says so).
 */
export interface MergeFailure {
  message: string
  checks: boolean
}

export function mergeFailure(error: unknown, fallback: string): MergeFailure {
  return {
    message: trpcErrorMessage(error, fallback),
    checks: trpcErrorCode(error) === MERGE_REFUSAL.checksCode,
  }
}
