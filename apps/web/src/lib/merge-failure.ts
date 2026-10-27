import { trpcErrorMessage } from "@/lib/trpc-error"

/**
 * EXP-1391: what a refused merge means for the UI: its message, nothing
 * else. Every refusal offers Retry; nothing reads a conflict any more (the
 * refusal went into the thread and its session takes it over, the message
 * says so).
 */
export interface MergeFailure {
  message: string
}

export function mergeFailure(error: unknown, fallback: string): MergeFailure {
  return { message: trpcErrorMessage(error, fallback) }
}
