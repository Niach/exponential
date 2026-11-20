import { createFileRoute } from "@tanstack/react-router"

// #EXP-2027: placeholder, filled in by the page's own commit.
export const Route = createFileRoute(`/t/$teamSlug/new/agent`)({
  component: NewAgentRoute,
})

function NewAgentRoute() {
  return null
}
