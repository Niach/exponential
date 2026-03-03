import { createFileRoute, redirect } from "@tanstack/react-router"

export const Route = createFileRoute(`/`)({
  beforeLoad: () => {
    throw redirect({ to: `/w/$workspaceSlug`, params: { workspaceSlug: `default` } })
  },
})
