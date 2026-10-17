import { createFileRoute } from "@tanstack/react-router"
import { TRPCError } from "@trpc/server"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { errorToResponse } from "@/lib/http-errors"
import { getHandoffBlob, putHandoffBlob } from "@/lib/sessions/handoff"

// EXP-1252: a moving session's handoff package (gzip tar: worktree, journal,
// the agent's native transcript). PUT = the SOURCE host while a move is
// pending (≤64 MiB, 413 above); GET = the session's host after the move
// (404 when there is none). The device's own credentials (API key / bearer).
async function userId(request: Request): Promise<string> {
  const session = await resolveSession(request)
  if (!session?.user) throw new TRPCError({ code: `UNAUTHORIZED`, message: `Unauthorized` })
  return session.user.id
}

export const Route = createFileRoute(`/api/sessions/$sessionId/handoff`)({
  server: {
    handlers: {
      PUT: async ({ params, request }) => {
        try {
          return await putHandoffBlob(await userId(request), params.sessionId, request)
        } catch (error) {
          return errorToResponse(error)
        }
      },
      GET: async ({ params, request }) => {
        try {
          return await getHandoffBlob(await userId(request), params.sessionId)
        } catch (error) {
          return errorToResponse(error)
        }
      },
    },
  },
})
