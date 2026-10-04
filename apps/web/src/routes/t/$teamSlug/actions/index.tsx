import { useEffect } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  TeamActionsPanel,
  type ActionsPanelTab,
} from "@/components/team-actions-panel"
import { useIsMobile } from "@exp/ui"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useCrossTeamScope } from "@/hooks/use-cross-team-scope"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { pageTitle } from "@/lib/page-title"

// The team Actions surface (EXP-686 — its own top-level route on every
// client). A desktop viewport shows the actions LIST alone (the suggestion
// seeds live in Getting started); mobile keeps the native-parity Actions ·
// Suggestions tabs, with the active one in `?tab=` so a back/refresh lands
// where the person was. An action itself is a page: `actions/$actionId`.
//
// EXP-694: `?editAction=` is the one-shot a session row's trailing button
// sends — since SLOP-2 it simply forwards to that action's page.
type ActionsSearch = {
  tab?: Exclude<ActionsPanelTab, `actions`>
  editAction?: string
}

export const Route = createFileRoute(`/t/$teamSlug/actions/`)({
  head: () => ({ meta: [{ title: pageTitle(`Actions`) }] }),
  validateSearch: (search: Record<string, unknown>): ActionsSearch => ({
    tab: search.tab === `suggestions` ? search.tab : undefined,
    editAction:
      typeof search.editAction === `string` && search.editAction !== ``
        ? search.editAction
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
  const { tab, editAction } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const isMobile = useIsMobile()
  // EXP-1186: the phone lists every member team's actions, one band per
  // team; md+ = the active team.
  const scope = useCrossTeamScope(team)

  useEffect(() => {
    if (!editAction) return
    void navigate({
      to: `/t/$teamSlug/actions/$actionId`,
      params: { teamSlug, actionId: editAction },
      replace: true,
    })
  }, [editAction, navigate, teamSlug])

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        {team ? (
          <TeamActionsPanel
            team={team}
            teams={scope.teams}
            grouped={scope.grouped}
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
