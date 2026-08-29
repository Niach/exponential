import { useEffect } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { TeamActionsPanel } from "@/components/team-actions-panel"
import { useIsMobile } from "@/hooks/use-mobile"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// The team Automations surface (EXP-686): its own sidebar entry and route on
// a desktop viewport. Mobile has no room for a seventh tab — there Automations
// is a tab of the Actions page, so this route replace-navigates there and the
// two clients agree on one destination for every "automations" link.
export const Route = createFileRoute(`/t/$teamSlug/automations`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: AutomationsPage,
})

function AutomationsPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const isMobile = useIsMobile()

  useEffect(() => {
    if (!isMobile) return
    void navigate({
      to: `/t/$teamSlug/actions`,
      params: { teamSlug },
      search: { tab: `automations` },
      replace: true,
    })
  }, [isMobile, navigate, teamSlug])

  if (isMobile) return null

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        {team ? (
          <TeamActionsPanel team={team} view="automations" />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">Loading…</div>
        )}
      </div>
    </div>
  )
}
