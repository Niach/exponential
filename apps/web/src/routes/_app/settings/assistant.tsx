import { createFileRoute } from "@tanstack/react-router"

// #EXP-2027: placeholder, filled in by the page's own commit.
export const Route = createFileRoute(`/_app/settings/assistant`)({
  component: AssistantRoute,
})

function AssistantRoute() {
  return null
}
