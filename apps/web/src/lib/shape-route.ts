import { resolveSession } from "@/lib/auth/resolve-bearer"
import { prepareElectricUrl, proxyElectricRequest } from "@/lib/electric-proxy"

interface ShapeRouteHandlerOptions {
  // userId is null when the request is anonymous. The shape proxy is expected
  // to return a where clause scoped to public-workspace data in that case.
  getWhere?: (userId: string | null) => Promise<string | null | undefined>
  table: string
  // If true, anonymous requests are rejected with 401 even when getWhere can
  // produce a where clause. Use for tables that only make sense for an
  // authenticated user (notifications, push subscriptions, etc).
  requireAuth?: boolean
}

export function createShapeRouteHandler({
  getWhere,
  table,
  requireAuth = false,
}: ShapeRouteHandlerOptions) {
  return async ({ request }: { request: Request }) => {
    // Auth accepts the session cookie (web), `Authorization: Bearer
    // <sessionToken>` (iOS, Android), legacy `expk_` api keys, or the desktop
    // agent's MCP OAuth2 access token — all via the single resolveSession
    // chokepoint. May be null for anonymous requests reading public data.
    const session = await resolveSession(request)

    if (!session && requireAuth) {
      return new Response(`Unauthorized`, { status: 401 })
    }

    const originUrl = prepareElectricUrl(request.url)
    originUrl.searchParams.set(`table`, table)

    const where = await getWhere?.(session?.user?.id ?? null)

    if (where) {
      originUrl.searchParams.set(`where`, where)
    }

    return proxyElectricRequest(originUrl, request.signal)
  }
}
