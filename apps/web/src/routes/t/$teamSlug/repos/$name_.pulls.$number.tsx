import { createFileRoute, redirect } from "@tanstack/react-router"

// #EXP-1616: an eCode PR's url (`ecodePullUrl`) opens its repo page on the
// Pull requests tab.
export const Route = createFileRoute(`/t/$teamSlug/repos/$name_/pulls/$number`)({
  beforeLoad: ({ params }) => {
    throw redirect({ to: `/t/$teamSlug/repos/$name`, params: { teamSlug: params.teamSlug, name: params.name }, search: { tab: `pulls` } })
  },
})
