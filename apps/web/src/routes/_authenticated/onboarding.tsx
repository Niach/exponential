import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useSession } from "@/hooks/use-session"
import { trpc } from "@/lib/trpc-client"
import { OnboardingWizard } from "@/components/onboarding/wizard"

export const Route = createFileRoute(`/_authenticated/onboarding`)({
  ssr: false,
  component: OnboardingPage,
})

function OnboardingPage() {
  const navigate = useNavigate()
  const { data: session } = useSession()
  const [workspace, setWorkspace] = useState<{
    id: string
    slug: string
  } | null>(null)

  useEffect(() => {
    if (!session?.user) return
    const user = session.user as { onboardingCompletedAt?: string | null }
    if (user.onboardingCompletedAt) {
      navigate({ to: `/w/$workspaceSlug`, params: { workspaceSlug: `default` } })
      return
    }
    void trpc.workspaces.ensureDefault
      .mutate()
      .then(({ workspace: ws }) => setWorkspace({ id: ws.id, slug: ws.slug }))
  }, [session, navigate])

  if (!workspace) return null

  return (
    <OnboardingWizard
      workspaceId={workspace.id}
      workspaceSlug={workspace.slug}
    />
  )
}
