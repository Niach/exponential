import { useEffect, useMemo, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  BoardGlyph,
  conceptIcon,
  EmptyState,
  JumpToBottomButton,
  SkeletonRows,
  useIsMobile,
} from "@exp/ui"
import type { User } from "@/db/schema"
import { SessionComposer } from "@/components/session-composer"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { AgentChannel, AgentComposer } from "@/components/thread/agent-channel"
import { AgentHeaderMenu } from "@/components/thread/chat-menu"
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
  const follow = useFollowScroll([messages, conversation?.id])
  const { users } = useTeamUsers(team?.id)
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
    <AgentHeaderMenu team={team} agentId={agent.id} title={name} className={className} />
  )
  const sendSurfaceReply = (text: string) => {
    const call = conversation
      ? trpc.sessions.send.mutate({ teamId: team.id, sessionId: conversation.id, text }, SKIP_ERROR_TOAST)
      : trpc.sessions.send.mutate({ agentId: agent.id, text }, SKIP_ERROR_TOAST)
    call.catch((error) => failedAction(`send`, `the message`, error))
  }
  const glyph = (
    <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-6" />
  )

  return (
    <div className="flex h-full min-h-0 flex-col max-md:h-dvh" data-testid="agent-channel-page">
      {isMobile ? (
        <MobileDetailHeader
          title={
            <span className="flex min-w-0 items-center gap-2">
              <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-4" />
              <span className="truncate">{name}</span>
            </span>
          }
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
              agent={agent}
              conversation={conversation}
              messages={messages}
              teamSessions={teamSessions}
              users={users as User[]}
              canAnswer={permissions.isMember}
              onOpenSession={openSession}
              layout={isMobile ? `bubbles` : `transcript`}
              onSurfaceReply={permissions.isMember ? sendSurfaceReply : undefined}
            />
          </div>
        </div>
        <JumpToBottomButton
          visible={!follow.atBottom && messages.length > 0}
          className="absolute bottom-3 left-1/2 z-10 -translate-x-1/2"
          onClick={follow.jumpToBottom}
        />
      </div>
      {permissions.isMember && (
        <div className="shrink-0 px-4 pt-2 pb-4 md:px-6">
          {conversation ? (
            <SessionComposer
              teamId={team.id}
              sessionId={conversation.id}
              placeholder={`Message ${name}`}
              users={users as User[]}
              busy={conversation.busy}
              canStop
              testId="agent-composer"
              className="mx-auto max-w-4xl"
            />
          ) : (
            <AgentComposer
              teamId={team.id}
              agentId={agent.id}
              placeholder={`Message ${name}`}
              users={users as User[]}
              testId="agent-composer"
              className="mx-auto max-w-4xl"
            />
          )}
        </div>
      )}
    </div>
  )
}
