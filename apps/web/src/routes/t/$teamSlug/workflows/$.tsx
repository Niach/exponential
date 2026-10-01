import { createFileRoute, redirect } from "@tanstack/react-router"

// SLOP-3: Workflows are gone; old links (`/workflows`, `/workflows/<id>`)
// land on the Agent page.
export const Route = createFileRoute(`/t/$teamSlug/workflows/$`)({
  beforeLoad: ({ params }) => {
    throw redirect({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug: params.teamSlug },
    })
  },
})
