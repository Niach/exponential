import { createFileRoute } from "@tanstack/react-router"
import { useAgents } from "@/hooks/use-session-tree"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { AgentEventsBand, AgentsListBand } from "@/components/team/agents-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/agents/`)({
  head: () => ({ meta: [{ title: pageTitle(`Agents`, `Settings`) }] }),
  component: SettingsAgentsIndex,
})

// EXP-1293 / NEW-2: the Agents page: the team's agents (owners get "New
// agent" on the band) over the team's Events, newest first. S-3: the
// viewer's Assistant first (created on first use here), others' never.
function SettingsAgentsIndex() {
  const { teamSlug } = Route.useParams()
  const { session, team, permissions, resolved } = useSettingsPage(teamSlug)
  const { agents, isReady } = useAgents(team?.id, session?.user?.id)
  useEnsureAssistant(team?.id)

  return (
    // Member-readable: every member sees the team's agents (read-only).
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && isReady && (
        <div className="space-y-6">
          <AgentsListBand team={team} isOwner={permissions.isOwner} agents={agents} />
          <AgentEventsBand teamId={team.id} />
        </div>
      )}
    </SettingsSectionGuard>
  )
}
