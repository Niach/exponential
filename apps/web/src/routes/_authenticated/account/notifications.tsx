import { createFileRoute, redirect } from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { clearLastVisited, readLastVisited } from "@/lib/last-visited"
import { trpc } from "@/lib/trpc-client"

// EXP-238 moved account settings into the unified team settings surface
// (Personal group). The old URL stays alive as a redirect because email
// verification links and muscle memory point here; it targets the
// Notifications section since the verify banner lives on that card.
//
// Resolution mirrors routes/index.tsx: last-visited team when the membership
// still holds, else the oldest membership via teams.getDefault (never
// creates), else onboarding. The /t/default magic slug is useless here — its
// guard drops deep paths on redirect.
export const Route = createFileRoute(`/_authenticated/account/notifications`)({
  beforeLoad: async () => {
    const last = readLastVisited()
    if (last) {
      let isMember = false
      try {
        const team = await trpc.teams.getBySlug.query({ slug: last.teamSlug })
        isMember = team.membership !== null
        if (!isMember) clearLastVisited()
      } catch (e) {
        const isNotFound =
          e instanceof TRPCClientError && e.data?.code === `NOT_FOUND`
        if (isNotFound) clearLastVisited()
        // Transient failures fall through to the default resolution.
      }
      if (isMember) {
        throw redirect({
          to: `/t/$teamSlug/settings/notifications`,
          params: { teamSlug: last.teamSlug },
        })
      }
    }

    const { team } = await trpc.teams.getDefault.query()
    if (team) {
      throw redirect({
        to: `/t/$teamSlug/settings/notifications`,
        params: { teamSlug: team.slug },
      })
    }
    throw redirect({ to: `/onboarding` })
  },
})
