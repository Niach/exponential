import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useSession } from "@/hooks/use-session"
import { hasCompletedOnboarding } from "@/lib/auth/app-user"
import { trpc } from "@/lib/trpc-client"
import {
  OnboardingWizard,
  type WizardEntryStep,
} from "@/components/onboarding/wizard"

// EXP-725: `?step=invite|devices` starts a resumed wizard past the board
// step. It is the shots pipeline's capture hook (the seeded starter owns a
// board-less team and must never create one) and is data-free: skipping the
// board stamps nothing, and `/` bounces a board-less user back here.
type OnboardingSearch = { step?: WizardEntryStep }

export const Route = createFileRoute(`/_authenticated/onboarding`)({
  ssr: false,
  validateSearch: (search: Record<string, unknown>): OnboardingSearch => ({
    step:
      search.step === `invite` || search.step === `devices`
        ? search.step
        : undefined,
  }),
  component: OnboardingPage,
})

// EXP-188: signups get no team, so the wizard starts at a create-or-join
// choice. We decide from TEAM EXISTENCE (teams.getDefault — never creates),
// not from the onboarding flag alone: the session cookie's flag can be up to
// 5 minutes stale, and a completed user who deleted their last team must
// land back on the choice step, not bounce forever.
function OnboardingPage() {
  const navigate = useNavigate()
  const { step } = Route.useSearch()
  const { data: session } = useSession()
  const [resolved, setResolved] = useState<{
    team: { id: string; slug: string } | null
  } | null>(null)

  useEffect(() => {
    if (!session?.user) return
    const completed = hasCompletedOnboarding(session.user)
    void trpc.teams.getDefault.query().then(({ team }) => {
      if (team && completed) {
        // Nothing left to onboard — go to the default team.
        navigate({ to: `/t/$teamSlug`, params: { teamSlug: team.slug } })
        return
      }
      // team + !completed → wizard resumes at the board step (the invite
      // and devices steps after it continue client-side and are skippable,
      // so nothing is lost by resuming there); no team → choice step (even
      // when the flag says completed — the deleted-last-team case).
      setResolved({
        team: team ? { id: team.id, slug: team.slug } : null,
      })
    })
  }, [session, navigate])

  if (!resolved) return null

  return <OnboardingWizard initialTeam={resolved.team} initialStep={step} />
}
