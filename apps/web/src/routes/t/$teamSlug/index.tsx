import { createFileRoute, redirect } from "@tanstack/react-router"

// The team index: its Teams page on a phone; md+ forwards that to Home
// (`widePathFor`), which has no active team (#EXP-2027, #EXP-2109).
export const Route = createFileRoute(`/t/$teamSlug/`)({
  beforeLoad: ({ params }) => {
    throw redirect({ to: `/t/$teamSlug/team`, params, replace: true })
  },
})
