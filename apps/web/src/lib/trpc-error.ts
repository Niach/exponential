import { TRPCClientError } from "@trpc/client"

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
  if (error instanceof TRPCClientError) {
    const message = error.message?.trim()
    if (message && !message.startsWith(`[`) && !message.startsWith(`{`)) {
      return message
    }
  }
  return fallback
}
