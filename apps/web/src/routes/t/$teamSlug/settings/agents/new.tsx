import { createFileRoute } from "@tanstack/react-router"
import { AgentSettingsPage } from "@/components/team/agents-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/agents/new`)({
  head: () => ({ meta: [{ title: pageTitle(`New agent`, `Settings`) }] }),
  component: SettingsNewAgent,
})

// EXP-1293: "New agent" = the agent page in CREATE mode — every field local
// until "Create agent" sends `agents.create`, then the page becomes the new
// agent's. Owner-only.
function SettingsNewAgent() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && <AgentSettingsPage team={team} agent={null} isOwner />}
    </SettingsSectionGuard>
  )
}
