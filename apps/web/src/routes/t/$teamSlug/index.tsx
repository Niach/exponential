import { createFileRoute, redirect } from "@tanstack/react-router"

// EXP-1253 (EXP-1259 D1): the Agent page is HOME. The team index always lands
// there (a replace, so Back never bounces through it); the last-visited board
// no longer decides the landing, and a board-less team's Getting started block
// lives on the home column now (composer-home.json).
export const Route = createFileRoute(`/t/$teamSlug/`)({
  beforeLoad: ({ params }) => {
    throw redirect({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug: params.teamSlug },
      replace: true,
    })
  },
})
