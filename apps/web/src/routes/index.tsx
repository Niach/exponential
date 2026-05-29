import { createFileRoute, redirect } from "@tanstack/react-router"
import { fetchSessionOnce } from "@/lib/auth/client"
import { hasCompletedOnboarding } from "@/lib/auth/app-user"

export const Route = createFileRoute(`/`)({
  ssr: false,
  beforeLoad: async () => {
    const sessionData = await fetchSessionOnce()
    if (sessionData?.user) {
      if (!hasCompletedOnboarding(sessionData.user)) {
        throw redirect({ to: `/onboarding` })
      }
    }
    throw redirect({
      to: `/w/$workspaceSlug`,
      params: { workspaceSlug: `default` },
    })
  },
})
