import { createFileRoute, redirect } from "@tanstack/react-router"

// #EXP-2027 / #EXP-2109: an old team-scoped link: the page is team-less.
export const Route = createFileRoute(`/t/$teamSlug/needs-you`)({
  validateSearch: (search: Record<string, unknown>): Record<string, unknown> => search,
  beforeLoad: ({ location }) => {
    throw redirect({ href: `/needs-you${location.searchStr}`, replace: true })
  },
})
