import { createFileRoute } from "@tanstack/react-router"
import { handleMonitoringTunnel } from "@/lib/observability/tunnel"
import { getMonitoringLimiter } from "@/lib/observability/limiter"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"

// #LOGS-2: the Sentry tunnel (lib/observability/tunnel.ts). Public: a crash
// before sign-in must still reach us. Small import surface (see
// routes/api/contact.ts); the auth graph loads only for a request carrying
// a credential, to key its rate limit on the user instead of the IP.
async function resolveUserId(request: Request): Promise<string | null> {
  const headers = request.headers
  if (!headers.get(`cookie`) && !headers.get(`authorization`)) return null
  const { auth } = await import(`@/lib/auth`)
  const session = await auth.api.getSession({ headers })
  return session?.user?.id ?? null
}

export const Route = createFileRoute(`/api/monitoring/`)({
  server: {
    handlers: {
      POST: ({ request }) =>
        handleMonitoringTunnel(request, {
          env: process.env,
          fetch: (url, init) => fetch(url, init),
          resolveUserId,
          clientIp: clientIpFromRequest,
          takeToken: (key) => getMonitoringLimiter().tryTake(key),
        }),
    },
  },
})
