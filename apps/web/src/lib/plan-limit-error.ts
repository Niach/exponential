import { TRPCClientError } from "@trpc/client"

// Single source of truth for how plan-limit errors travel from server to
// client: every limit throw in lib/billing.ts uses tRPC code
// `PRECONDITION_FAILED` and a message starting with this prefix. Clients use
// isPlanLimitError() to distinguish "you hit a plan cap → show an upgrade
// nudge" from other precondition failures (e.g. "GitHub App not installed").
export const PLAN_LIMIT_MESSAGE_PREFIX = `Your plan allows`

export function isPlanLimitError(err: unknown): boolean {
  return (
    err instanceof TRPCClientError &&
    err.data?.code === `PRECONDITION_FAILED` &&
    typeof err.message === `string` &&
    err.message.startsWith(PLAN_LIMIT_MESSAGE_PREFIX)
  )
}
