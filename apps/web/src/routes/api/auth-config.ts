import { createFileRoute } from "@tanstack/react-router"
import { buildAuthConfig } from "@/lib/auth-config"

export const Route = createFileRoute(`/api/auth-config`)({
  server: {
    handlers: {
      GET: () => Response.json(buildAuthConfig()),
    },
  },
})
