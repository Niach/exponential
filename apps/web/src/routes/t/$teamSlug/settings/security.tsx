import { createFileRoute, redirect } from "@tanstack/react-router"

// #EXP-2027 / #EXP-2109: an old team-scoped link: the personal settings are
// team-less.
export const Route = createFileRoute(`/t/$teamSlug/settings/security`)({
  validateSearch: (search: Record<string, unknown>): Record<string, unknown> => search,
  beforeLoad: ({ location }) => {
    throw redirect({ href: `/settings/security${location.searchStr}`, replace: true })
  },
})
