import { createFileRoute } from "@tanstack/react-router"
import { preflightResponse } from "@/lib/widget/cors"
import { handleWidgetSubmit } from "@/lib/widget/service"

// Public, cross-origin: the widget panel posts every report here. The
// pipeline lives in lib/widget/service.ts — keep this file's imports
// identical to config.ts (see the note in service.ts about dev route
// registration).
export const Route = createFileRoute(`/api/widget/submit`)({
  server: {
    handlers: {
      POST: ({ request }) => handleWidgetSubmit(request),
      // Permissive preflight echo — see config.ts for rationale.
      OPTIONS: ({ request }) =>
        preflightResponse(request.headers.get(`origin`)),
    },
  },
})
