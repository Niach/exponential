import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  TeamActionsPanel,
  type ActionsPanelTab,
} from "@/components/team-actions-panel"
import { useIsMobile } from "@/hooks/use-mobile"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// The team Actions surface (EXP-686 — its own top-level route on every
// client). A desktop viewport shows the actions LIST alone (automations are
// `/automations`, the suggestion seeds moved into Getting started); mobile
// keeps the native-parity Actions · Automations · Suggestions tabs, with the
// active one in `?tab=` so a back/refresh lands where the person was.
type ActionsSearch = { tab?: Exclude<ActionsPanelTab, `actions`> }

export const Route = createFileRoute(`/t/$teamSlug/actions`)({
  validateSearch: (search: Record<string, unknown>): ActionsSearch => ({
    tab:
      search.tab === `automations` || search.tab === `suggestions`
        ? search.tab
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
  component: ActionsPage,
})

function ActionsPage() {
  const { teamSlug } = Route.useParams()
  const { tab } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const isMobile = useIsMobile()

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        {team ? (
          <TeamActionsPanel
            team={team}
            view={isMobile ? `tabs` : `actions`}
            tab={tab ?? `actions`}
            onTabChange={(next) =>
              void navigate({
                to: `/t/$teamSlug/actions`,
                params: { teamSlug },
                search: next === `actions` ? {} : { tab: next },
                replace: true,
              })
            }
          />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">Loading…</div>
        )}
      </div>
    </div>
  )
}
