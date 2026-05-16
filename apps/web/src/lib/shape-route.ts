import { auth } from "@/lib/auth"
import { prepareElectricUrl, proxyElectricRequest } from "@/lib/electric-proxy"

interface ShapeRouteHandlerOptions {
  getWhere?: (userId: string) => Promise<string | null | undefined>
  table: string
}

export function createShapeRouteHandler({
  getWhere,
  table,
}: ShapeRouteHandlerOptions) {
  return async ({ request }: { request: Request }) => {
    // Auth on /api/shapes/* is intentionally a single branch: better-auth's
    // `bearer()` plugin (registered in lib/auth.ts) makes `getSession` accept
    // both the session cookie (web) and `Authorization: Bearer <token>` (iOS
    // and Android). One auth call covers all three clients — do not split.
    const session = await auth.api.getSession({
      headers: request.headers,
    })

    if (!session) {
      return new Response(`Unauthorized`, { status: 401 })
    }

    const originUrl = prepareElectricUrl(request.url)
    originUrl.searchParams.set(`table`, table)

    const where = await getWhere?.(session.user.id)

    if (where) {
      originUrl.searchParams.set(`where`, where)
    }

    return proxyElectricRequest(originUrl, request.signal)
  }
}
