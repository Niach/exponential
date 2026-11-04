import { createFileRoute } from "@tanstack/react-router"
import { shareTargetFallback } from "@/lib/share/share-target-fallback"

// EXP-1635: the PWA manifest's `share_target` action. The service worker
// answers it (`public/sw.js`); this is the browser with no worker yet.
export const Route = createFileRoute(`/api/share-target`)({
  server: {
    handlers: {
      POST: async ({ request }) => shareTargetFallback(request),
    },
  },
})
