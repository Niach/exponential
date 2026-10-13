import { createFileRoute } from "@tanstack/react-router"

// EXP-1293: placeholder, filled in by its lane.
export const Route = createFileRoute(`/t/$teamSlug/settings/agents/$agentId`)({
  component: () => null,
})
