import { useEffect, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { BoardGlyph, conceptIcon, EmptyState, SkeletonRows, useIsMobile } from "@exp/ui"
import type { User } from "@/db/schema"
import { SessionComposer } from "@/components/session-composer"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { AgentChannel } from "@/components/thread/agent-channel"
import { useSessionMessages, useSessionRow, useTeamSessions } from "@/hooks/use-session-tree"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { THREADS_LIST_COPY, type SessionRow } from "@/lib/session-tree"

// EXP-1293 Threads v2: a team agent's CHANNEL (the rail's Agents section,
// the phones' Agents band): the agent icon (colored) + name, then the
// channel's comment rows and THE session cards of what it started, and the
// session composer (`Message <name>`) posting into the agent root.

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
  const { row: agent, isReady } = useSessionRow(agentId)
  const messages = useSessionMessages(agentId)
  const { rows: teamSessions } = useTeamSessions(team?.id)
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

  if (!team || !agent || agent.kind !== `agent`) {
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

  const name = agent.name ?? ``
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
        />
      ) : (
        <div className="flex shrink-0 items-center gap-3 border-b border-border px-6 pt-6 pb-5 md:px-10">
          {glyph}
          <h1 className="min-w-0 truncate text-lg font-semibold tracking-tight md:text-2xl">
            {name}
          </h1>
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col justify-end px-4 pt-8 pb-8 md:px-6">
          <AgentChannel
            agent={agent}
            messages={messages}
            teamSessions={teamSessions}
            users={users as User[]}
            canAnswer={permissions.isMember}
            onOpenSession={openSession}
          />
        </div>
      </div>
      {permissions.isMember && (
        <div className="shrink-0 px-4 pt-2 pb-4 md:px-6">
          <SessionComposer
            teamId={team.id}
            sessionId={agent.id}
            placeholder={`Message ${name}`}
            users={users as User[]}
            busy={agent.busy}
            canStop
            testId="agent-composer"
            className="mx-auto max-w-3xl"
          />
        </div>
      )}
    </div>
  )
}
