import { TRPCClientError } from "@trpc/client"

/**
 * EXP-533: the ONE sentence a transport failure reads as, byte-identical on
 * web, iOS, Android and desktop. Chrome's "Failed to fetch", Safari's "Load
 * failed" and Android's "Unable to resolve host" are all the same fact, and
 * none of them is something a user can act on.
 */
export const OFFLINE_ERROR_MESSAGE = `You're offline. Check your connection and try again.`

/**
 * A tRPC call that never reached the server. The httpBatchLink wraps a failed
 * `fetch` in a `TRPCClientError` with NO `data` (there was no response to
 * shape it) and the underlying `TypeError` as its cause.
 *
 * An AbortError is not an outage (a navigation or a superseded query aborted
 * it on purpose), so it is deliberately excluded.
 */
export function isTransportError(error: unknown): boolean {
  if (!(error instanceof TRPCClientError)) return false
  if (error.data !== undefined) return false
  const cause: unknown = error.cause
  if (!(cause instanceof Error)) return false
  if (cause.name === `AbortError`) return false
  return cause.name === `TypeError`
}

/**
 * The server's TRPCError message when it reads like a sentence — the authz and
 * integration layers write human copy ("Not a member of this team", GitHub's
 * verbatim "Pull Request is not mergeable"). Zod validation errors serialize
 * as a JSON array/object, so those fall back instead.
 *
 * The sibling of `serverErrorDetail` in `trpc-client.ts` (which additionally
 * caps the length for a toast); this one is for call sites rendering the
 * message inline, where the full text is what the user needs.
 */
export function trpcErrorMessage(error: unknown, fallback: string): string {
  if (isTransportError(error)) return OFFLINE_ERROR_MESSAGE
  if (error instanceof TRPCClientError) {
    const message = error.message?.trim()
    if (message && !message.startsWith(`[`) && !message.startsWith(`{`)) {
      return message
    }
  }
  return fallback
}

/** The server's TRPCError code (`FORBIDDEN`, `NOT_FOUND`, …) when the error is
 *  a tRPC one; undefined for network failures and anything else. */
export function trpcErrorCode(error: unknown): string | undefined {
  if (!(error instanceof TRPCClientError)) return undefined
  const code = (error.data as { code?: unknown } | undefined)?.code
  return typeof code === `string` ? code : undefined
}
