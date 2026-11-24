import { createFileRoute, redirect } from "@tanstack/react-router"

// #EXP-2027 / #EXP-2109: an old team-scoped link: the page is team-less.
export const Route = createFileRoute(`/t/$teamSlug/chats/`)({
  validateSearch: (search: Record<string, unknown>): Record<string, unknown> => search,
  beforeLoad: ({ location }) => {
    throw redirect({ href: `/chats${location.searchStr}`, replace: true })
  },
})
