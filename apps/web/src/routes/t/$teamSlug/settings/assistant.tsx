import { createFileRoute } from "@tanstack/react-router"
import { ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"
import { EmptyState, SkeletonRows, conceptIcon } from "@exp/ui"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSession } from "@/hooks/use-session"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { AgentSettingsPage } from "@/components/team/agents-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/assistant`)({
  head: () => ({
    meta: [{ title: pageTitle(ASSISTANT_COPY.title, `Settings`) }],
  }),
  component: SettingsAssistant,
})

const DevicesIcon = conceptIcon(`nav-devices`)

// S-3 (settings-nav.json `@assistant`, agent-settings.json `assistant`): the
// viewer's OWN Assistant's page. It resolves the Assistant through
// `agents.ensureAssistant` (created on first use on the member's newest own
// desktop) and renders the agent page's Assistant variant over the LIVE
// synced row; with no device yet it reads `copy.noDevice`.
function SettingsAssistant() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const assistantId = useEnsureAssistant(team?.id)
  const { agent: row } = useAgentRow(assistantId ?? null)
  // #EXP-1714: the global Assistant lives in the member's personal team.
  const agent = row && row.role === `assistant` && row.ownerId === me ? row : null

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {assistantId === null ? (
        <EmptyState icon={DevicesIcon} title={ASSISTANT_COPY.noDevice} size="list" />
      ) : team && agent ? (
        <AgentSettingsPage key={agent.id} team={team} agent={agent} isOwner={permissions.isOwner} />
      ) : (
        <SkeletonRows className="py-1" />
      )}
    </SettingsSectionGuard>
  )
}
