import { createFileRoute } from "@tanstack/react-router"
import { versionPayload } from "@/lib/client-version"

// Permanently-stable, unauthenticated client-version advertisement: the
// per-platform minimum (below which the API answers 426) and latest
// versions, straight from the CLIENT_MIN_VERSION_* / CLIENT_LATEST_VERSION_*
// env vars (null = unset = no gate). Even the oldest client must be able to
// call this forever — keep it dependency-free and never change its shape
// incompatibly.
export const Route = createFileRoute(`/api/version`)({
  server: {
    handlers: {
      GET: () =>
        Response.json(versionPayload(), {
          headers: { "cache-control": `no-store` },
        }),
    },
  },
})
