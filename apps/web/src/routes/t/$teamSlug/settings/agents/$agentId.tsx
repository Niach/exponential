import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useAgentRow } from "@/hooks/use-agent-row"
import { AgentSettingsPage } from "@/components/team/agents-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle, usePageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/agents/$agentId`)({
  head: () => ({
    meta: [{ title: pageTitle(`Agents`, `Settings`) }],
  }),
  component: SettingsAgent,
})

// EXP-1293 / NEW-2: one team agent's settings PAGE (the board page's layout),
// fed the LIVE synced `agents` row (the route id IS the agent id), so every write reflects via sync and an agent deleted
// out from under the page simply stops rendering. Members read it; the
// owner-only controls are hidden inside.
function SettingsAgent() {
  const { teamSlug, agentId } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { agent: row, isReady } = useAgentRow(agentId)
  const agent = row && row.teamId === team?.id ? row : null
  usePageTitle(agent?.name ? pageTitle(agent.name, `Settings`) : undefined)
  const navigate = useNavigate()

  // An id that resolves to no agent once the snapshot is in (a stale link,
  // an agent deleted meanwhile) lands on the index: the first agent or the
  // empty state, never a blank page.
  useEffect(() => {
    if (!isReady || !team || agent) return
    void navigate({
      to: `/t/$teamSlug/settings/agents`,
      params: { teamSlug },
      replace: true,
    })
  }, [agent, isReady, navigate, team, teamSlug])

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && agent && (
        <AgentSettingsPage
          key={agent.id}
          team={team}
          agent={agent}
          isOwner={permissions.isOwner}
        />
      )}
    </SettingsSectionGuard>
  )
}
