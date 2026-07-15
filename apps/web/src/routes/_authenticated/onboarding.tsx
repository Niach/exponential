import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useSession } from "@/hooks/use-session"
import { hasCompletedOnboarding } from "@/lib/auth/app-user"
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
    if (hasCompletedOnboarding(session.user)) {
      navigate({ to: `/t/$workspaceSlug`, params: { workspaceSlug: `default` } })
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
