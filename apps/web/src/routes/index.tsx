import { createFileRoute, redirect } from "@tanstack/react-router"
import { authClient } from "@/lib/auth/client"

export const Route = createFileRoute(`/`)({
  ssr: false,
  beforeLoad: async () => {
    const result = await authClient.getSession()
    if (result.data?.user) {
      const user = result.data.user as { onboardingCompletedAt?: string | null }
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
