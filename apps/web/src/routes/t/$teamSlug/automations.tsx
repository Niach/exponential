import { useEffect, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { TeamActionsPanel } from "@/components/team-actions-panel"
import { useIsMobile } from "@/hooks/use-mobile"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// The team Automations surface (EXP-686): its own sidebar entry and route on
// a desktop viewport. Mobile has no room for a seventh tab — there Automations
// is a tab of the Actions page, so this route replace-navigates there and the
// two clients agree on one destination for every "automations" link.
//
// EXP-694: `?editAutomation=` is the one-shot editor request a session row's
// trailing button carries (handed over by the Actions route on a desktop
// viewport, where the tab it targets does not exist).
type AutomationsSearch = { editAutomation?: string }

export const Route = createFileRoute(`/t/$teamSlug/automations`)({
  validateSearch: (search: Record<string, unknown>): AutomationsSearch => ({
    editAutomation:
      typeof search.editAutomation === `string` && search.editAutomation !== ``
        ? search.editAutomation
        : undefined,
  }),
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
  const { editAutomation } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const isMobile = useIsMobile()

  // The one-shot editor request, held after the URL key is stripped.
  const [editAutomationId, setEditAutomationId] = useState<string | null>(null)

  useEffect(() => {
    if (!isMobile) return
    void navigate({
      to: `/t/$teamSlug/actions`,
      params: { teamSlug },
      search: editAutomation
        ? { tab: `automations`, editAutomation }
        : { tab: `automations` },
      replace: true,
    })
  }, [isMobile, editAutomation, navigate, teamSlug])

  useEffect(() => {
    if (isMobile || !editAutomation) return
    setEditAutomationId(editAutomation)
    void navigate({
      to: `/t/$teamSlug/automations`,
      params: { teamSlug },
      search: {},
      replace: true,
    })
  }, [isMobile, editAutomation, navigate, teamSlug])

  if (isMobile) return null

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        {team ? (
          <TeamActionsPanel
            team={team}
            view="automations"
            editAutomationId={editAutomationId}
            onEditAutomationConsumed={() => setEditAutomationId(null)}
          />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">Loading…</div>
        )}
      </div>
    </div>
  )
}
