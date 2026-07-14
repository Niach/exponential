import { createFileRoute } from "@tanstack/react-router"
import { buildAppleAppSiteAssociation } from "@/lib/app-links"

// iOS Universal Links (EXP-92). A server route (not a public/ static file)
// because the file is extensionless — Nitro's static handler would serve it
// without the application/json content-type Apple requires. Served on every
// instance; Apple only fetches domains listed in the app's entitlement.
export const Route = createFileRoute(`/.well-known/apple-app-site-association`)(
  {
    server: {
      handlers: {
        GET: () =>
          new Response(JSON.stringify(buildAppleAppSiteAssociation()), {
            headers: {
              "Content-Type": `application/json`,
              "Cache-Control": `public, max-age=3600`,
            },
          }),
      },
    },
  }
)
