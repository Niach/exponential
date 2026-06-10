import { createFileRoute } from "@tanstack/react-router"
import { preflightResponse } from "@/lib/widget/cors"
import { handleWidgetConfig } from "@/lib/widget/service"

// Public, cross-origin: the widget loader fetches this to configure the
// button/form before anything renders on the host page. The pipeline lives
// in lib/widget/service.ts — keep this file's imports identical to
// submit.ts (see the note in service.ts about dev route registration).
export const Route = createFileRoute(`/api/widget/config`)({
  server: {
    handlers: {
      GET: ({ request }) => handleWidgetConfig(request),
      // Preflights can't carry the key (no body, and enforcing here would
      // grant nothing — the actual request is re-checked), so echo the
      // requesting origin permissively. Credentials are never allowed.
      OPTIONS: ({ request }) =>
        preflightResponse(request.headers.get(`origin`)),
    },
  },
})
