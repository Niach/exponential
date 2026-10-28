import { useEffect, useMemo, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  BoardGlyph,
  CHAT_HEADER_AGENT_SUBTITLE,
  conceptIcon,
  EmptyState,
  ExponentialLogo,
  FloatingComposer,
  PhoneHeaderTitle,
  SkeletonRows,
  ThreadDock,
  useIsMobile,
} from "@exp/ui"
import { isAssistant } from "@exp/domain-contract/agent-settings"
import type { User } from "@/db/schema"
import { SessionComposer } from "@/components/session-composer"
import {
  HEADER_BUTTON_CLASS,
  MobileDetailHeader,
} from "@/components/team/mobile-detail-header"
import { renderThreadText } from "@/components/thread/thread-markdown"
import {
  ThreadScreen,
  validateThreadSearch,
  type ThreadScreenAgent,
} from "@/components/thread/thread-screen"
import { AgentHeaderMenu } from "@/components/thread/chat-menu"
import { CallButton, useVoiceCall, useVoiceCallAvailability, VoiceCallOverlay } from "@/components/voice-call"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSession } from "@/hooks/use-session"
import { useSessionThread, useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { THREADS_LIST_COPY } from "@/lib/session-tree"

// EXP-1293 / NEW-2 / S-3 (chats.json CHAT MODE): a team agent's chat. The
// route id IS the `agents` row; the page is THE thread screen
// (`components/thread/thread-screen.tsx`, the same component a thread
// renders) over the CALLER's own conversation session with it (a root with
// that `agentId`, `userId` = me), in chat mode `bubbles` (`threadChatMode`):
// its header carries the agent's disc + name (the Assistant: THE brand
// mark), the call button and THE agent menu; the composer is the thread's
// with the + menu of surface `agent`. The only wrapper of its own is for an
// agent with NO conversation yet: the same header, an empty body, and the
// same composer whose first send is `sessions.send({agentId})`.

const AgentsIcon = conceptIcon(`settings-agents`)

/** How long a missing agent waits for its row before "not found". */
const SYNC_GRACE_MS = 4_000

export const Route = createFileRoute(`/t/$teamSlug/agents/$agentId`)({
  head: () => ({ meta: [{ title: pageTitle(THREADS_LIST_COPY.agents) }] }),
  validateSearch: validateThreadSearch,
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
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const { data: authSession } = useSession()
  const me = authSession?.user?.id ?? null
  const { agent: agentRow, isReady } = useAgentRow(agentId)
  const agent =
    agentRow && agentRow.teamId === team?.id && (!isAssistant(agentRow) || agentRow.ownerId === me)
      ? agentRow
      : null
  const { rows: teamSessions } = useTeamSessions(team?.id)
  const conversation = useMemo(
    () =>
      teamSessions.find(
        (row) => row.agentId === agentId && row.userId === me && row.parentId === null
      ) ?? null,
    [teamSessions, agentId, me]
  )
  const thread = useSessionThread(
    conversation?.id ?? null,
    search.tab === `session` ? (search.session ?? null) : null
  )
  const { users } = useTeamUsers(team?.id)
  // NEW-70 (voice-call.json): THE telephone left of `…`, the call card over the chat.
  const callAvailability = useVoiceCallAvailability()
  const voiceCall = useVoiceCall(agentId)
  usePageTitle(agent?.name ? pageTitle(agent.name, THREADS_LIST_COPY.agents) : undefined)

  const [graceOver, setGraceOver] = useState(false)
  useEffect(() => {
    setGraceOver(false)
    const timer = setTimeout(() => setGraceOver(true), SYNC_GRACE_MS)
    return () => clearTimeout(timer)
  }, [agentId])

  if (!team || !agent) {
    if (team && isReady && graceOver) {
      return (
        <div className="flex h-full flex-col items-center justify-center">
          <EmptyState
            icon={AgentsIcon}
            title="Not found"
            description="It may still be syncing, or it was deleted."
            size="page"
          />
        </div>
      )
    }
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }

  const name = agent.name
  const assistant = isAssistant(agent)
  const disc = (className: string, glyphClass: string) => (
    <span
      data-slot="chat-lead"
      className={`flex shrink-0 items-center justify-center rounded-full bg-muted ${className}`}
    >
      {assistant ? (
        <ExponentialLogo variant="light" className={`h-auto text-foreground ${glyphClass}`} />
      ) : (
        <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className={glyphClass} />
      )}
    </span>
  )
  const parts: ThreadScreenAgent = {
    name,
    glyph: disc(`size-8`, `size-4`),
    mobileTitle: (
      <PhoneHeaderTitle
        disc={disc(`size-10`, `size-5`)}
        title={name}
        subtitle={<span className="text-muted-foreground">{CHAT_HEADER_AGENT_SUBTITLE}</span>}
      />
    ),
    headerMenu: (className?: string) => (
      <>
        <CallButton
          availability={callAvailability}
          agentName={name}
          active={voiceCall.call.active}
          onCall={() => void voiceCall.call.start(agent.id, callAvailability.device)}
          className={className}
        />
        <AgentHeaderMenu team={team} agentId={agent.id} title={name} className={className} />
      </>
    ),
    overlay: <VoiceCallOverlay {...voiceCall} />,
  }

  if (conversation && thread.view && thread.root) {
    return (
      <ThreadScreen
        key={thread.root.id}
        team={team}
        teamSlug={teamSlug}
        view={thread.view}
        root={thread.root}
        rows={thread.rows}
        titles={thread.titles}
        pullRequests={thread.pullRequests}
        messagePage={thread.messagePage}
        search={search}
        onNavigate={(next, options) =>
          void navigate({
            to: `/t/$teamSlug/agents/$agentId`,
            params: { teamSlug, agentId },
            search: next,
            replace: options?.replace,
          })
        }
        agent={parts}
      />
    )
  }
  if (conversation) {
    // The conversation's tree is still syncing.
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }

  // No conversation yet: THE thread screen's header over an empty body, the
  // same composer (its first send opens the conversation).
  const composer = permissions.isMember ? (
    <SessionComposer
      teamId={team.id}
      sessionId={null}
      agentId={agent.id}
      surface="agent"
      placeholder={`Message ${name}`}
      users={users as User[]}
      opaque={isMobile}
      testId="agent-composer"
    />
  ) : null

  return (
    <div className="relative flex h-full min-h-0 flex-col max-md:h-dvh" data-testid="agent-channel-page">
      {isMobile ? (
        <MobileDetailHeader
          title={parts.mobileTitle}
          onBack={() => void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug } })}
          menu={parts.headerMenu(HEADER_BUTTON_CLASS)}
        />
      ) : (
        <div className="flex shrink-0 items-center gap-3 border-b border-border px-6 pt-6 pb-5 md:px-10">
          {parts.glyph}
          <h1 className="min-w-0 flex-1 truncate text-2xl font-semibold tracking-tight">{name}</h1>
          {parts.headerMenu(`shrink-0 text-muted-foreground`)}
        </div>
      )}
      {parts.overlay}
      <div className="relative min-h-0 flex-1">
        <div data-testid="thread-scroll" className="h-full" />
        {composer && !isMobile && (
          <ThreadDock
            rows={[]}
            sessionName={() => null}
            renderText={renderThreadText}
            onAnswer={() => undefined}
            onDecide={() => undefined}
            composer={composer}
            onHeight={() => undefined}
          />
        )}
      </div>
      {composer && isMobile && (
        <FloatingComposer placeholder={`Message ${name}`}>{composer}</FloatingComposer>
      )}
    </div>
  )
}
