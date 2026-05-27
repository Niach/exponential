import { createFileRoute, redirect } from "@tanstack/react-router"
import { fetchSessionOnce } from "@/lib/auth/client"

export const Route = createFileRoute(`/`)({
  ssr: false,
  beforeLoad: async () => {
    const sessionData = await fetchSessionOnce()
    if (sessionData?.user) {
      const user = sessionData.user as { onboardingCompletedAt?: string | null }
      if (!user.onboardingCompletedAt) {
        throw redirect({ to: `/onboarding` })
      }
    }
    throw redirect({
      to: `/w/$workspaceSlug`,
      params: { workspaceSlug: `default` },
    })
  },
})
