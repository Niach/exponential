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
  const [team, setTeam] = useState<{
    id: string
    slug: string
  } | null>(null)

  useEffect(() => {
    if (!session?.user) return
    if (hasCompletedOnboarding(session.user)) {
      navigate({ to: `/t/$teamSlug`, params: { teamSlug: `default` } })
      return
    }
    void trpc.teams.ensureDefault
      .mutate()
      .then(({ team: ws }) => setTeam({ id: ws.id, slug: ws.slug }))
  }, [session, navigate])

  if (!team) return null

  return (
    <OnboardingWizard
      teamId={team.id}
      teamSlug={team.slug}
    />
  )
}
