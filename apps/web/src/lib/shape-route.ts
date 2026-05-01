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
