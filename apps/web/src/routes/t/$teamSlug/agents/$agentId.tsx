import { createFileRoute, Navigate, redirect, useNavigate } from "@tanstack/react-router"
import { isAssistant } from "@exp/domain-contract/agent-settings"
import { useIsMobile } from "@exp/ui"
import { AgentChannel } from "@/components/chat/agent-channel"
import { validateChatSearch } from "@/components/chat/chat-screen"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSession } from "@/hooks/use-session"
import { pageTitle } from "@/lib/page-title"
import { CHATS_LIST_COPY } from "@/lib/session-tree"

// A team agent's chat (or, on md+, the Assistant's): `AgentChannel`. #EXP-2109:
// on a phone the viewer's Assistant is a TAB, so its link opens that tab.

export const Route = createFileRoute(`/t/$teamSlug/agents/$agentId`)({
  head: () => ({ meta: [{ title: pageTitle(CHATS_LIST_COPY.agents) }] }),
  validateSearch: validateChatSearch,
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: AgentChannelPage,
})

function AgentChannelPage() {
  const { teamSlug, agentId } = Route.useParams()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const { data: authSession } = useSession()
  const { agent } = useAgentRow(isMobile ? agentId : null)
  if (isMobile && agent && isAssistant(agent) && agent.ownerId === authSession?.user?.id) {
    return <Navigate to="/assistant" search={search} replace />
  }
  return (
    <AgentChannel
      teamSlug={teamSlug}
      agentId={agentId}
      search={search}
      onNavigate={(next, options) =>
        void navigate({
          to: `/t/$teamSlug/agents/$agentId`,
          params: { teamSlug, agentId },
          search: next,
          replace: options?.replace,
        })
      }
    />
  )
}
