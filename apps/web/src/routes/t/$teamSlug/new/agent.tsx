import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { Separator } from "@exp/ui"
import { AgentSettingsPage } from "@/components/team/agents-section"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { NEW_AGENT_LABEL, useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json TEAM MENU): New › Agent opens this PAGE, the
// agent page in CREATE mode (EXP-1293, what Settings › Agents › New agent
// renders) outside settings. Owner-only: anyone else lands on Home.
export const Route = createFileRoute(`/t/$teamSlug/new/agent`)({
  head: () => ({ meta: [{ title: pageTitle(NEW_AGENT_LABEL) }] }),
  component: NewAgentRoute,
})

function NewAgentRoute() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const refused = resolved && !permissions.isOwner
  useEffect(() => {
    if (refused) void navigate({ to: `/home`, replace: true })
  }, [refused, navigate])
  if (!team || !resolved || refused) return null
  return (
    <div className={`mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${PAGE_END_CLEARANCE}`}>
      <h1 className="text-2xl font-bold">{NEW_AGENT_LABEL}</h1>
      <Separator />
      <AgentSettingsPage team={team} agent={null} isOwner />
    </div>
  )
}
