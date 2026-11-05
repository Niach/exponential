import { createFileRoute } from "@tanstack/react-router"
import { isDesktopOs, resolveDesktopDownload } from "@/lib/desktop-download-resolver"

// EXP-1693: the desktop download buttons (web, site, the desktop card). A
// redirect to the newest desktop-v* release that HAS this OS's asset: public,
// unauthenticated, cached a few minutes.
export const Route = createFileRoute(`/api/download/desktop/$os`)({
  server: {
    handlers: {
      GET: async ({ params }) => {
        if (!isDesktopOs(params.os)) return new Response(null, { status: 404 })
        const { url, fromRelease } = await resolveDesktopDownload(params.os)
        return new Response(null, {
          status: 302,
          headers: {
            location: url,
            // The GitHub-down fallback is a guess: never let it stick.
            "cache-control": fromRelease ? `public, max-age=300` : `no-store`,
          },
        })
      },
    },
  },
})
