import { useEffect, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  TeamActionsPanel,
  type ActionsPanelTab,
} from "@/components/team-actions-panel"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { useIsMobile } from "@/hooks/use-mobile"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useSteerConfig } from "@/components/agent-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// The team Actions surface (EXP-686 — its own top-level route on every
// client). A desktop viewport shows the actions LIST alone (automations are
// `/automations`, the suggestion seeds moved into Getting started); mobile
// keeps the native-parity Actions · Automations · Suggestions tabs, with the
// active one in `?tab=` so a back/refresh lands where the person was.
//
// EXP-694: three one-shot requests ride the URL the same way the board route's
// `?new=1` compose does — `?chat=1` is the mobile FAB's chat launcher (the
// Devices page's pattern), and `?editAction=`/`?editAutomation=` open an
// editor straight from a session row's trailing button. Each is consumed once
// and stripped so a back/refresh never re-opens the dialog.
type ActionsSearch = {
  tab?: Exclude<ActionsPanelTab, `actions`>
  chat?: 1
  editAction?: string
  editAutomation?: string
}

export const Route = createFileRoute(`/t/$teamSlug/actions`)({
  validateSearch: (search: Record<string, unknown>): ActionsSearch => ({
    tab:
      search.tab === `automations` || search.tab === `suggestions`
        ? search.tab
        : undefined,
    chat: search.chat === 1 || search.chat === `1` ? 1 : undefined,
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
  const { tab, chat, editAction, editAutomation } = Route.useSearch()
  const navigate = useNavigate()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const isMobile = useIsMobile()

  // The mobile FAB's Chat launcher — the same dialog the device rows open,
  // on its Chat tab with no device preference.
  const [chatOpen, setChatOpen] = useState(false)
  // The one-shot editor requests, held after the URL key is stripped.
  const [editActionId, setEditActionId] = useState<string | null>(null)
  const [editAutomationId, setEditAutomationId] = useState<string | null>(null)

  const currentUserId = session?.user?.id
  const teamId = team?.id
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId,
    teamId,
  })

  // Consume the one-shots, then drop their keys (the tab stays, so a
  // `?tab=automations&editAutomation=…` link lands on the right tab). A
  // desktop viewport has no Automations tab here — that surface is its own
  // route, so the request is handed over instead of dropped.
  useEffect(() => {
    if (!chat && !editAction && !editAutomation) return
    if (editAutomation && !isMobile) {
      void navigate({
        to: `/t/$teamSlug/automations`,
        params: { teamSlug },
        search: { editAutomation },
        replace: true,
      })
      return
    }
    if (chat) setChatOpen(true)
    if (editAction) setEditActionId(editAction)
    if (editAutomation) setEditAutomationId(editAutomation)
    void navigate({
      to: `/t/$teamSlug/actions`,
      params: { teamSlug },
      search: tab ? { tab } : {},
      replace: true,
    })
  }, [chat, editAction, editAutomation, isMobile, tab, navigate, teamSlug])

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

      {team && (
        <LaunchDialog
          open={chatOpen}
          onOpenChange={(next) => {
            if (!next) setChatOpen(false)
          }}
          devices={remote.devices ?? []}
          starting={remote.starting}
          teamId={team.id}
          initialTab="chat"
          onStartIssues={(device, options, issueIds) => {
            remote
              .startIssues(device, options, issueIds)
              .then(() => setChatOpen(false))
              .catch(() => {})
          }}
          onRunAction={(device, action, options, inputs) => {
            remote
              .runAction(device, action, options, inputs)
              .then(() => setChatOpen(false))
              .catch(() => {})
          }}
        />
      )}
    </div>
  )
}
