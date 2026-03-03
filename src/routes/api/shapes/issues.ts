import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import {
  prepareElectricUrl,
  proxyElectricRequest,
} from "@/lib/electric-proxy"

export const Route = createFileRoute(`/api/shapes/issues`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const session = await auth.api.getSession({
          headers: request.headers,
        })
        if (!session) {
          return new Response(`Unauthorized`, { status: 401 })
        }

        const originUrl = prepareElectricUrl(request.url)
        originUrl.searchParams.set(`table`, `issues`)

        return proxyElectricRequest(originUrl)
      },
    },
  },
})
