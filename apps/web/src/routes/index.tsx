import { createFileRoute, redirect } from "@tanstack/react-router"
import { TRPCClientError } from "@trpc/client"
import { fetchSessionOnce } from "@/lib/auth/client"
import { hasCompletedOnboarding } from "@/lib/auth/app-user"
import { clearLastVisited, readLastVisited } from "@/lib/last-visited"
import { trpc } from "@/lib/trpc-client"

export const Route = createFileRoute(`/`)({
  ssr: false,
  beforeLoad: async () => {
    const sessionData = await fetchSessionOnce()
    if (sessionData?.user) {
      if (!hasCompletedOnboarding(sessionData.user)) {
        throw redirect({ to: `/onboarding` })
      }

      // EXP-69: jump back to this device's last-used workspace (the workspace
      // index then prefers the last-used project board). The helper is
      // window-guarded, so a non-browser evaluation just skips this. A stale
      // entry — workspace deleted or membership lost — clears itself and
      // falls through to the /w/default resolution below.
      const last = readLastVisited()
      if (last) {
        let isMember = false
        try {
          const workspace = await trpc.workspaces.getBySlug.query({
            slug: last.workspaceSlug,
          })
          isMember = workspace.membership !== null
          if (!isMember) clearLastVisited()
        } catch (e) {
          const isNotFound =
            e instanceof TRPCClientError && e.data?.code === `NOT_FOUND`
          if (isNotFound) clearLastVisited()
          // Transient failures (offline, 500) keep the entry and fall
          // through to the default resolution.
        }
        if (isMember) {
          throw redirect({
            to: `/w/$workspaceSlug`,
            params: { workspaceSlug: last.workspaceSlug },
          })
        }
      }
    }
    throw redirect({
      to: `/w/$workspaceSlug`,
      params: { workspaceSlug: `default` },
    })
  },
})
