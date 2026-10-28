import { useEffect, useMemo, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { threadSessionNames } from "@exp/domain-contract/ask-dock"
import {
  BoardGlyph,
  conceptIcon,
  EmptyState,
  FloatingComposer,
  JumpToBottomButton,
  JUMP_TO_BOTTOM_GAP,
  SkeletonRows,
  ThreadDock,
  useIsMobile,
} from "@exp/ui"
import type { ReactNode } from "react"
import type { User } from "@/db/schema"
import { SessionComposer, SessionComposerReplies } from "@/components/session-composer"
import { SessionLiveTail, useSessionDock } from "@/components/agent-session"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import {
  AgentChannel,
  AgentComposer,
  useAgentChannelRows,
  useAnswerMessage,
} from "@/components/thread/agent-channel"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { AgentHeaderMenu } from "@/components/thread/chat-menu"
import { AgentHeaderTitle } from "@/components/thread/chat-header"
import { CallButton, useVoiceCall, useVoiceCallAvailability, VoiceCallOverlay } from "@/components/voice-call"
import { HEADER_BUTTON_CLASS } from "@/components/team/mobile-detail-header"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { useFollowScroll } from "@/hooks/use-follow-scroll"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSession } from "@/hooks/use-session"
import { useTeamSessions, useTreeMessages } from "@/hooks/use-session-tree"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { THREADS_LIST_COPY, type SessionRow } from "@/lib/session-tree"

// EXP-1293 / NEW-2: a team agent's page (the rail's Agents section, the
// phones' Agents band). The route id IS the `agents` row; the page shows the
// CALLER's own conversation session with it (a root with that `agentId`,
// `userId` = me): the agent icon (colored) + name, the conversation's comment
// rows and THE session cards of what it started, and the composer
// (`Message <name>`). No conversation yet = the same screen, empty, whose
// composer opens one with `sessions.send({agentId})`.
// Thread 58cb32df (chats.json MENUS + TRANSCRIPT): the header's `…` = THE
// agent menu (Compact conversation · New conversation · Stop while busy ·
// Agent settings for owners); no Stop pill; phones paint chat bubbles.

const AgentsIcon = conceptIcon(`settings-agents`)

/** How long a missing agent waits for its row before "not found". */
const SYNC_GRACE_MS = 4_000

export const Route = createFileRoute(`/t/$teamSlug/agents/$agentId`)({
  head: () => ({ meta: [{ title: pageTitle(THREADS_LIST_COPY.agents) }] }),
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
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const { data: authSession } = useSession()
  const me = authSession?.user?.id ?? null
  const { agent: agentRow, isReady } = useAgentRow(agentId)
  const agent = agentRow && agentRow.teamId === team?.id ? agentRow : null
  const { rows: teamSessions } = useTeamSessions(team?.id)
  const conversation = useMemo(
    () =>
      teamSessions.find(
        (row) => row.agentId === agentId && row.userId === me && row.parentId === null
      ) ?? null,
    [teamSessions, agentId, me]
  )
  // The conversation's whole tree: a child's question / plan card surfaces
  // here and answers in place (`sessions.answer`, straight to that child).
  const messages = useTreeMessages(conversation?.id)
  const { rows, tabs } = useAgentChannelRows({ conversation, messages, teamSessions })
  // #EXP-1407 (ask-dock.json, dock.json): one in-flight set for a card
  // answered inline or in its Dock sheet.
  const answering = useAnswerMessage()
  const sessionName = useMemo(() => threadSessionNames(tabs), [tabs])
  const follow = useFollowScroll([messages, conversation?.id])
  const [dockHeight, setDockHeight] = useState(0)
  const { atBottom, jumpToBottom } = follow
  // FEED-140 (the thread's rule): a pinned reader stays pinned as the band grows.
  useEffect(() => {
    if (atBottom) jumpToBottom()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dockHeight])
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

  const openSession = (session: SessionRow) =>
    void (session.parentId === null
      ? navigate({
          to: `/t/$teamSlug/threads/$threadId`,
          params: { teamSlug, threadId: session.id },
        })
      : navigate({
          to: `/t/$teamSlug/sessions/$sessionId`,
          params: { teamSlug, sessionId: session.id },
        }))

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
  const headerMenu = (className?: string) => (
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
  )
  const sendSurfaceReply = (text: string) => {
    const call = conversation
      ? trpc.sessions.send.mutate({ teamId: team.id, sessionId: conversation.id, text }, SKIP_ERROR_TOAST)
      : trpc.sessions.send.mutate({ agentId: agent.id, text }, SKIP_ERROR_TOAST)
    call.catch((error) => failedAction(`send`, `the message`, error))
  }
  const composer = conversation ? (
    <SessionComposer
      teamId={team.id}
      sessionId={conversation.id}
      placeholder={`Message ${name}`}
      users={users as User[]}
      busy={conversation.busy}
      canStop
      opaque={isMobile}
      hideReplies={isMobile}
      testId="agent-composer"
    />
  ) : (
    <AgentComposer
      teamId={team.id}
      agentId={agent.id}
      placeholder={`Message ${name}`}
      users={users as User[]}
      opaque={isMobile}
      testId="agent-composer"
    />
  )
  const glyph = (
    <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-6" />
  )

  return (
    <div className="relative flex h-full min-h-0 flex-col max-md:h-dvh" data-testid="agent-channel-page">
      {isMobile ? (
        <MobileDetailHeader
          // Chats (the draft, iOS/Android): the agent's disc, the name over `Agent`.
          title={<AgentHeaderTitle name={name} icon={agent.icon} color={agent.color} />}
          onBack={() => void navigate({ to: `/t/$teamSlug/threads`, params: { teamSlug } })}
          menu={headerMenu(HEADER_BUTTON_CLASS)}
        />
      ) : (
        <div className="flex shrink-0 items-center gap-3 border-b border-border px-6 pt-6 pb-5 md:px-10">
          {glyph}
          <h1 className="min-w-0 flex-1 truncate text-lg font-semibold tracking-tight md:text-2xl">
            {name}
          </h1>
          {headerMenu(`shrink-0 text-muted-foreground`)}
        </div>
      )}
      <VoiceCallOverlay {...voiceCall} />
      {/* EXP-1316 (1n): the session feed's scroll (opens at the newest
          row, follows while pinned, THE jump button once scrolled up). */}
      <div className="relative min-h-0 flex-1">
        <div
          ref={follow.scrollRef}
          onScroll={follow.onScroll}
          data-testid="agent-channel-scroll"
          className="h-full overflow-y-auto overscroll-contain"
        >
          <div
            ref={follow.contentRef}
            className="mx-auto flex min-h-full w-full max-w-4xl flex-col justify-end px-4 pt-8 pb-8 md:px-6"
          >
            <AgentChannel
              // #EXP-1380: another agent = a fresh transcript window (the pinned
              // oldest row belongs to THIS chat).
              key={agentId}
              agent={agent}
              conversation={conversation}
              allRows={rows}
              answering={answering}
              messages={messages}
              teamSessions={teamSessions}
              users={users as User[]}
              canAnswer={permissions.isMember}
              onOpenSession={openSession}
              layout={isMobile ? `bubbles` : `transcript`}
              onSurfaceReply={permissions.isMember ? sendSurfaceReply : undefined}
              scrollRef={follow.scrollRef}
              tail={
                // #EXP-1380 (chats.json WORKING): the run view's ONE status
                // line while the conversation works.
                conversation && conversation.status !== `ended` ? (
                  <SessionLiveTail session={conversation} />
                ) : undefined
              }
            />
            {/* #EXP-1407: the Dock's band height as a spacer (the follow
                scroll's content observer re-pins on it, padding would not). */}
            <div aria-hidden className="shrink-0" style={{ height: dockHeight }} />
          </div>
        </div>
        {/* #EXP-1383 (owner: "exactly as in sessions"): the run view's
            circle, placement and layer; the Dock sits right below. */}
        <JumpToBottomButton
          visible={!follow.atBottom && messages.length > 0}
          className="absolute left-1/2 z-30 -translate-x-1/2"
          style={{ bottom: dockHeight + JUMP_TO_BOTTOM_GAP }}
          onClick={follow.jumpToBottom}
        />
        {/* #EXP-1407 (dock.json): THE Dock over the conversation's bottom
            edge, exactly as in a thread: the tree's waiting plan / question
            cards (caption = the asking session, the root = none), the
            root's Queue, queued messages and banners (the agent hint),
            then the composer, in ONE column inset on md+. FEED-160: a
            phone's band carries no composer, only the work bar's clearance
            (THE floating capsule below), exactly as the thread page. */}
        {permissions.isMember &&
          (conversation ? (
            <AgentConversationDock
              conversation={conversation}
              rows={rows}
              sessionName={sessionName}
              answering={answering}
              onOpenSession={(id) => {
                const target = teamSessions.find((row) => row.id === id)
                if (target) openSession(target)
              }}
              composer={isMobile ? undefined : composer}
              phoneClearance={isMobile}
              onHeight={setDockHeight}
            />
          ) : (
            <ThreadDock
              rows={[]}
              sessionName={sessionName}
              renderText={renderThreadText}
              onAnswer={() => undefined}
              onDecide={() => undefined}
              composer={isMobile ? undefined : composer}
              phoneClearance={isMobile}
              onHeight={setDockHeight}
            />
          ))}
      </div>
      {/* EXP-1316: phones reply through THE floating composer capsule. */}
      {permissions.isMember && isMobile && (
        <FloatingComposer
          placeholder={`Message ${name}`}
          above={conversation ? <SessionComposerReplies sessionId={conversation.id} /> : undefined}
        >
          {composer}
        </FloatingComposer>
      )}
    </div>
  )
}

/** #EXP-1407: the conversation's Dock = THE thread Dock (`ThreadDock`) over
 *  its rows, with the root's own sources (`useSessionDock`: the Queue,
 *  queued messages, banners incl. the agent hint). */
function AgentConversationDock({
  conversation,
  rows,
  sessionName,
  answering,
  onOpenSession,
  composer,
  phoneClearance,
  onHeight,
}: {
  conversation: SessionRow
  rows: Parameters<typeof ThreadDock>[0][`rows`]
  sessionName: (sessionId: string) => string | null
  answering: ReturnType<typeof useAnswerMessage>
  onOpenSession: (sessionId: string) => void
  composer?: ReactNode
  phoneClearance?: boolean
  onHeight: (height: number) => void
}) {
  const sessionDock = useSessionDock(conversation)
  const { answer, pending } = answering
  return (
    <ThreadDock
      rows={rows}
      sessionName={sessionName}
      renderText={renderThreadText}
      pending={pending}
      onAnswer={(id, input) => void answer(id, input)}
      onDecide={(id, input) => void answer(id, input)}
      sources={sessionDock.sources}
      renderSheet={sessionDock.renderSheet}
      renderAction={sessionDock.renderAction}
      onOpenSession={onOpenSession}
      composer={composer}
      phoneClearance={phoneClearance}
      onHeight={onHeight}
      autoOpenReady={rows.length > 0}
    />
  )
}
