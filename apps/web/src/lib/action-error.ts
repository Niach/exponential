import { actionErrorTitle, toastActionError } from "@exp/ui"
import { TRPCClientError } from "@trpc/client"
import { isTransportError, OFFLINE_ERROR_MESSAGE } from "@/lib/trpc-error"

// FEED-79 (EXP-1259 D10): the web's ONE failed-action surface. Every site
// that used to print a rose sentence after a failed save/delete/start/
// connect/merge/send/invite calls `failedAction` instead. A tRPC mutation
// already raises the generic `mutationErrorToastLink` toast, so a site that
// toasts its own (named title, Retry) passes `SKIP_ERROR_TOAST` on the call.

/** `mutate(input, SKIP_ERROR_TOAST)`: this site raises its own toast. */
export const SKIP_ERROR_TOAST = { context: { skipErrorToast: true } } as const

/** The server's message when it reads like a sentence (a tRPC message, a
 *  Better Auth `{ message }`, a thrown Error); offline reads as offline. */
export function actionErrorDescription(error: unknown): string | undefined {
  if (error === null || error === undefined) return undefined
  if (typeof error === `string`) return error.trim() || undefined
  if (isTransportError(error)) return OFFLINE_ERROR_MESSAGE
  const message =
    error instanceof TRPCClientError || error instanceof Error
      ? error.message
      : typeof error === `object` && `message` in error && typeof error.message === `string`
        ? error.message
        : undefined
  const trimmed = message?.trim()
  if (!trimmed || trimmed.startsWith(`[`) || trimmed.startsWith(`{`)) return undefined
  return trimmed
}

/** Toast a failed action: `Couldn't <verb> <thing>` + the server's message,
 *  Retry when `retry` is given (only when re-running the call is safe).
 *  Returns the toast id. */
export function failedAction(
  verb: string,
  thing: string,
  error?: unknown,
  options: { retry?: () => void; retryLabel?: string; description?: string } = {}
): string | number {
  return toastActionError(actionErrorTitle(verb, thing), {
    description: options.description ?? actionErrorDescription(error),
    retry: options.retry,
    ...(options.retryLabel ? { retryLabel: options.retryLabel } : {}),
  })
}
