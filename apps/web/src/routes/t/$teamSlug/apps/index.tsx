import { createFileRoute, redirect } from "@tanstack/react-router"

// #EXP-2109: the phones' Apps tab is gone: a team's apps are rows of its
// Teams page (md+ has them in the sidebar; that page forwards to Home).
export const Route = createFileRoute(`/t/$teamSlug/apps/`)({
  beforeLoad: ({ params }) => {
    throw redirect({ to: `/t/$teamSlug/team`, params, replace: true })
  },
})
