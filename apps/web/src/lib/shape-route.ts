import { auth } from "@/lib/auth"
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
    // Auth accepts either the session cookie (web) or `Authorization: Bearer`
    // (iOS, Android, MCP) via better-auth's bearer plugin. May be null for
    // anonymous requests reading public-workspace data.
    const session = await auth.api.getSession({
      headers: request.headers,
    })

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
