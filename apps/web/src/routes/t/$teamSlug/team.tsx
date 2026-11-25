import { createFileRoute, redirect } from "@tanstack/react-router"

// #EXP-2109 (sidebar-tree.json `phone`): a link to a team = the phones' Teams
// tab with that team unfolded. md+ forwards to Home (`widePathFor`).
export const Route = createFileRoute(`/t/$teamSlug/team`)({
  beforeLoad: ({ params }) => {
    throw redirect({ to: `/teams`, search: { team: params.teamSlug }, replace: true })
  },
})
