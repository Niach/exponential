import { useEffect, useState } from "react"
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
//
// EXP-694: two one-shot requests ride the URL the same way the board route's
// `?new=1` compose does — `?editAction=`/`?editAutomation=` open an editor
// straight from a session row's trailing button. Each is consumed once and
// stripped so a back/refresh never re-opens the dialog. (EXP-739 retired the
// `?chat=1` launcher: the mobile FAB links to `/t/$teamSlug/chat`.)
type ActionsSearch = {
  tab?: Exclude<ActionsPanelTab, `actions`>
  editAction?: string
  editAutomation?: string
}

export const Route = createFileRoute(`/t/$teamSlug/actions`)({
  validateSearch: (search: Record<string, unknown>): ActionsSearch => ({
    tab:
      search.tab === `automations` || search.tab === `suggestions`
        ? search.tab
        : undefined,
    editAction:
      typeof search.editAction === `string` && search.editAction !== ``
        ? search.editAction
        : undefined,
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
  component: ActionsPage,
})

function ActionsPage() {
  const { teamSlug } = Route.useParams()
  const { tab, editAction, editAutomation } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const isMobile = useIsMobile()

  // The one-shot editor requests, held after the URL key is stripped.
  const [editActionId, setEditActionId] = useState<string | null>(null)
  const [editAutomationId, setEditAutomationId] = useState<string | null>(null)

  // Consume the one-shots, then drop their keys (the tab stays, so a
  // `?tab=automations&editAutomation=…` link lands on the right tab). A
  // desktop viewport has no Automations tab here — that surface is its own
  // route, so the request is handed over instead of dropped.
  useEffect(() => {
    if (!editAction && !editAutomation) return
    if (editAutomation && !isMobile) {
      void navigate({
        to: `/t/$teamSlug/automations`,
        params: { teamSlug },
        search: { editAutomation },
        replace: true,
      })
      return
    }
    if (editAction) setEditActionId(editAction)
    if (editAutomation) setEditAutomationId(editAutomation)
    void navigate({
      to: `/t/$teamSlug/actions`,
      params: { teamSlug },
      search: tab ? { tab } : {},
      replace: true,
    })
  }, [editAction, editAutomation, isMobile, tab, navigate, teamSlug])

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
            editActionId={editActionId}
            onEditActionConsumed={() => setEditActionId(null)}
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
