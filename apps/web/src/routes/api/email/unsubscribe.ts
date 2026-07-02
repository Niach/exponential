import { createFileRoute } from "@tanstack/react-router"
import { handleUnsubscribe } from "@/lib/email-unsubscribe"

// One-click unsubscribe target embedded in every notification email
// (List-Unsubscribe header + footer link). Public by design: the opaque token
// is the auth. Supports GET (footer link) and POST (RFC 8058 one-click from
// mail clients). The DB module is lazy-imported (health.ts pattern) so loading
// this route module never eagerly opens a DB pool.
async function serve(request: Request): Promise<Response> {
  const token = new URL(request.url).searchParams.get(`token`)
  const { unsubscribeByToken } = await import(`@/lib/notification-prefs`)
  return await handleUnsubscribe(token, unsubscribeByToken)
}

export const Route = createFileRoute(`/api/email/unsubscribe`)({
  server: {
    handlers: {
      GET: ({ request }) => serve(request),
      POST: ({ request }) => serve(request),
    },
  },
})
