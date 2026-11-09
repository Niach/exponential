import { createFileRoute } from "@tanstack/react-router"
import {
  isMonitoringPlatform,
  monitoringConfig,
} from "@/lib/observability/config"
import { isProductionBuild } from "@/lib/production-build"

// #LOGS-2: THE diagnostics config every client asks for
// (`?platform=web|desktop|cli|ede|ios|android`): `{ enabled: true, dsn,
// environment, tracesSampleRate }` or `{ enabled: false }` (unconfigured =
// every self-hosted and dev instance). Public, env-derived, cached an hour.
export const Route = createFileRoute(`/api/monitoring/config`)({
  server: {
    handlers: {
      GET: ({ request }) => {
        const platform = new URL(request.url).searchParams.get(`platform`)
        if (!isMonitoringPlatform(platform)) {
          return Response.json(
            { error: `platform must be one of web, desktop, cli, ede, ios, android` },
            { status: 400 }
          )
        }
        return Response.json(
          monitoringConfig(process.env, platform, isProductionBuild),
          { headers: { "cache-control": `public, max-age=3600` } }
        )
      },
    },
  },
})
