import { useEffect, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  BoardGlyph,
  conceptIcon,
  EmptyState,
  ExponentialLogo,
  FloatingComposer,
  SkeletonRows,
  ChatDock,
  useIsMobile,
} from "@exp/ui"
import { isAssistant, triggerSummary, type AgentTriggerLike } from "@exp/domain-contract/agent-settings"
import { sessionScreen, sessionScreenKind, sessionScreenSubtitle } from "@exp/domain-contract/session-screen"
import type { User } from "@/db/schema"
import { SessionComposer } from "@/components/session-composer"
import { HEADER_BUTTON_CLASS } from "@/components/team/mobile-detail-header"
import { renderChatText } from "@/components/chat/chat-markdown"
import {
  ChatScreen,
  validateChatSearch,
  type ChatScreenAgent,
  type ChatScreenLinkMenu,
  type ChatSearch,
} from "@/components/chat/chat-screen"
import { AgentHeaderMenu } from "@/components/chat/chat-menu"
import { CallButton, useVoiceCall, useVoiceCallAvailability, VoiceCallOverlay } from "@/components/voice-call"
import { useAgentRow, useMyAgentConversation } from "@/hooks/use-agent-row"
import { useSession } from "@/hooks/use-session"
import { useSessionChat } from "@/hooks/use-session-tree"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { CHATS_LIST_COPY } from "@/lib/session-tree"
import { useSessionNav } from "@/hooks/use-session-nav"
import { useResultsTab } from "@/hooks/use-results-tab"
import { AgentSettingsPage } from "@/components/team/agents-section"
import { SessionScreenFrame } from "@/components/chat/session-screen-frame"
import { SessionIssuesTab, SessionResultsTab, SessionSessionsTab } from "@/components/chat/session-screen-bodies"

// EXP-1293 / NEW-2 / #EXP-1732 (session-screen.json): a team agent's chat
// (or the Assistant's). The route id IS the `agents` row; the page is THE
// session screen (`components/chat/chat-screen.tsx`, the same one a chat
// renders) over the CALLER's own conversation session with it (a root with
// that `agentId`, `userId` = me): the agent's disc (the Assistant: THE
// brand mark), its name over `status · device · trigger`, the call button
// and THE session screen menu (#FEED-239: its Agent settings opens the
// settings body as the item, `open=settings`; no gear);
// the tabs Chat · Issues · Sessions · Results. With NO conversation yet the
// same frame draws an empty chat whose composer's first send is
// `sessions.send({agentId})`.

const AgentsIcon = conceptIcon(`settings-agents`)

/** How long a missing agent waits for its row before "not found". */
const SYNC_GRACE_MS = 4_000

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
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const { data: authSession } = useSession()
  const me = authSession?.user?.id ?? null
  const { agent: agentRow, isReady } = useAgentRow(agentId)
  // #EXP-1714: the viewer's Assistant opens from any team (the global one
  // lives in their personal team); a team agent only in its own.
  const agent =
    agentRow && (isAssistant(agentRow) ? agentRow.ownerId === me : agentRow.teamId === team?.id) ? agentRow : null
  const conversation = useMyAgentConversation(agentId, me)
  const chat = useSessionChat(
    conversation?.id ?? null,
    search.open === `session` ? (search.session ?? null) : null
  )
  // The empty screen's Results (no tree yet = none).
  const emptyResults = useResultsTab(team?.id, null)
  const { users } = useTeamUsers(team?.id)
  // S-3: the back stack while no conversation exists yet (the session
  // screen keeps its own once it does).
  const nav = useSessionNav({ teamSlug, here: null, back: search.back })
  // NEW-70 (voice-call.json): THE telephone left of `…`, the call card over the chat.
  const callAvailability = useVoiceCallAvailability()
  const voiceCall = useVoiceCall(agentId)
  usePageTitle(agent?.name ? pageTitle(agent.name, CHATS_LIST_COPY.agents) : undefined)

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
  const avatar = (
    <span
      data-slot="chat-lead"
      className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted"
    >
      {assistant ? (
        <ExponentialLogo variant="light" className="h-auto size-5 text-foreground" />
      ) : (
        <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-5" />
      )}
    </span>
  )
  // session-screen.json: the subtitle's trigger = the first enabled one.
  const triggers = (Array.isArray(agent.triggers) ? agent.triggers : []) as AgentTriggerLike[]
  const firstTrigger = triggers.find((trigger) => trigger && trigger.enabled !== false)
  const trigger = firstTrigger ? triggerSummary(firstTrigger) : null
  const buttonClass = isMobile ? HEADER_BUTTON_CLASS : `shrink-0 text-muted-foreground`
  const parts: ChatScreenAgent = {
    name,
    assistant,
    avatar,
    trigger,
    trailing: (
      <CallButton
        availability={callAvailability}
        agentName={name}
        active={voiceCall.call.active}
        onCall={() => void voiceCall.call.start(agent.id, callAvailability.device, callAvailability.language)}
        className={buttonClass}
      />
    ),
    menu: (className: string, onSettings: () => void, link?: ChatScreenLinkMenu) => (
      <AgentHeaderMenu
        team={team}
        agentId={agent.id}
        title={name}
        onSettings={onSettings}
        linkIssue={link?.rows}
        closeRef={link?.closeRef}
        className={className}
      />
    ),
    overlay: (
      <VoiceCallOverlay
        {...voiceCall}
        agentName={name}
        avatar={
          assistant ? (
            <ExponentialLogo variant="light" />
          ) : (
            <BoardGlyph board={{ icon: agent.icon, color: agent.color }} />
          )
        }
      />
    ),
    settings: <AgentSettingsPage key={agent.id} team={team} agent={agent} isOwner={permissions.isOwner} />,
  }
  const go = (next: ChatSearch, options?: { replace?: boolean }) =>
    void navigate({
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug, agentId },
      search: next,
      replace: options?.replace,
    })

  if (conversation && chat.view && chat.root) {
    return (
      <ChatScreen
        key={chat.root.id}
        team={team}
        teamSlug={teamSlug}
        view={chat.view}
        root={chat.root}
        rows={chat.rows}
        titles={chat.titles}
        pullRequests={chat.pullRequests}
        messagePage={chat.messagePage}
        search={search}
        onNavigate={go}
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

  // No conversation yet: THE session screen's frame over an empty chat, the
  // same composer (its first send opens the conversation).
  const screen = sessionScreen({
    kind: sessionScreenKind({ agentId: agent.id, appId: null, assistant }),
    wide: !isMobile,
    requested: search.tab ?? null,
    open: search.open === `settings` ? `settings` : null,
    counts: { issues: 0, sessions: 0, results: 0 },
  })
  const keepBack = search.back ? { back: search.back } : {}
  const composer = permissions.isMember ? (
    <SessionComposer
      teamId={agent.teamId}
      browseTeamId={team.id}
      sessionId={null}
      agentId={agent.id}
      surface="agent"
      placeholder={`Message ${name}`}
      users={users as User[]}
      opaque={isMobile}
      testId="agent-composer"
    />
  ) : null
  const emptyChat = (
    <div className="relative flex min-h-0 flex-1 flex-col" data-testid="agent-channel-page">
      <div className="relative min-h-0 flex-1">
        <div data-testid="chat-scroll" className="h-full" />
        {composer && !isMobile && (
          <ChatDock
            rows={[]}
            sessionName={() => null}
            renderText={renderChatText}
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
  const pane =
    screen.active === `issues` ? (
      <SessionIssuesTab issues={[]} activeId={null} onOpen={() => undefined} />
    ) : screen.active === `sessions` ? (
      <SessionSessionsTab rows={[]} activeId={null} onOpen={() => undefined} />
    ) : screen.active === `results` ? (
      <SessionResultsTab
        teamId={team.id}
        teamSlug={teamSlug}
        results={emptyResults}
        pullRequest={null}
        canMerge={permissions.isMember}
        review={false}
        file={null}
        onReview={() => undefined}
      />
    ) : null

  return (
    <SessionScreenFrame
      screen={screen}
      testId="chat-page"
      avatar={avatar}
      title={name}
      subtitle={sessionScreenSubtitle({ status: null, device: null, trigger })}
      trailing={parts.trailing}
      menu={(className) => parts.menu(className, () => go({ tab: screen.active, open: `settings`, ...keepBack }))}
      onBack={nav.pop ?? (() => void navigate({ to: `/t/$teamSlug/chats`, params: { teamSlug } }))}
      onPop={nav.pop}
      onTab={(tab) => go({ tab, ...keepBack }, { replace: true })}
      chat={emptyChat}
      pane={pane}
      item={
        screen.open === `settings` ? (
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain" data-testid="session-settings">
            <div className="mx-auto w-full max-w-3xl px-4 pt-4 pb-10">{parts.settings}</div>
          </div>
        ) : null
      }
      onCloseItem={() => go({ tab: screen.active, ...keepBack }, { replace: true })}
      overlay={parts.overlay}
    />
  )
}
