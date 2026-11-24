import { createFileRoute, Navigate, useNavigate } from "@tanstack/react-router"
import { ASSISTANT_COPY, isAssistant } from "@exp/domain-contract/agent-settings"
import { EmptyState, SkeletonRows, conceptIcon, useIsMobile } from "@exp/ui"
import { AgentChannel } from "@/components/chat/agent-channel"
import { validateChatSearch } from "@/components/chat/chat-screen"
import { useAllAgentRows } from "@/hooks/use-agent-row"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { pageTitle } from "@/lib/page-title"

// #EXP-2109 (chats.json `shell`): the phones' ASSISTANT tab, the viewer's one
// Assistant conversation; md+ opens the same chat as its own page.
export const Route = createFileRoute(`/_app/assistant`)({
  head: () => ({ meta: [{ title: pageTitle(ASSISTANT_COPY.title) }] }),
  validateSearch: validateChatSearch,
  component: AssistantRoute,
})

const AgentsIcon = conceptIcon(`settings-agents`)

function AssistantRoute() {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const synced = useAllAgentRows().find((agent) => isAssistant(agent) && agent.ownerId === user.id)
  // The first use creates it (S-3); null = none yet (no device).
  const ensured = useEnsureAssistant(null)
  const agentId = synced?.id ?? ensured
  if (!agentId) {
    return agentId === null ? (
      <div className="flex h-full flex-col items-center justify-center">
        <EmptyState icon={AgentsIcon} title={ASSISTANT_COPY.title} description={ASSISTANT_COPY.noDevice} size="page" />
      </div>
    ) : (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }
  if (!isMobile) {
    return <Navigate to="/t/$teamSlug/agents/$agentId" params={{ teamSlug: defaultTeamSlug, agentId }} search={search} replace />
  }
  return (
    <AgentChannel
      key={agentId}
      tab
      teamSlug={defaultTeamSlug}
      agentId={agentId}
      search={search}
      onNavigate={(next, options) => void navigate({ to: `/assistant`, search: next, replace: options?.replace })}
    />
  )
}
