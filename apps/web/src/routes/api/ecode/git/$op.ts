import { createFileRoute } from "@tanstack/react-router"
import { handleGitRequest } from "@/lib/ecode/git-routes"

// #EXP-1616: eCode's own git wire (`lib/ecode/git-routes.ts`). Bearer = a
// repo token (git-remote-exp, device jobs), never a session cookie.
export const Route = createFileRoute(`/api/ecode/git/$op`)({
  server: {
    handlers: {
      GET: ({ request, params }) => handleGitRequest(request, params.op),
      POST: ({ request, params }) => handleGitRequest(request, params.op),
    },
  },
})
