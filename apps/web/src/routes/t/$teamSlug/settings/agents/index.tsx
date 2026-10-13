import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useAgents } from "@/hooks/use-session-tree"
import { AgentsEmptyState } from "@/components/team/agents-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/agents/`)({
  head: () => ({ meta: [{ title: pageTitle(`Agents`, `Settings`) }] }),
  component: SettingsAgentsIndex,
})

// EXP-1293: like Boards, the Agents section has no list page — the settings
// nav IS the list. Bare /settings/agents forwards to the first agent; a team
// with none renders the empty state (owners get "New agent" on its band).
function SettingsAgentsIndex() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { agents, isReady } = useAgents(team?.id)
  const first = agents[0]

  useEffect(() => {
    if (!first) return
    void navigate({
      to: `/t/$teamSlug/settings/agents/$agentId`,
      params: { teamSlug, agentId: first.sessionId },
      replace: true,
    })
  }, [first, navigate, teamSlug])

  return (
    // Member-readable: every member sees the team's agents (read-only).
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && isReady && !first && (
        <AgentsEmptyState team={team} isOwner={permissions.isOwner} />
      )}
    </SettingsSectionGuard>
  )
}
