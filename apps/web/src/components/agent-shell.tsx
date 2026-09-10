import { type ReactNode } from "react"
import { useParams } from "@tanstack/react-router"
import type { CodingSession } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { nestSessions } from "@/lib/session-tree"
import { sessionIdentity } from "@/lib/session-identity"
import { cn } from "@/lib/utils"
import { relativeTime } from "@/components/comment-rows/format"
import { RunningIndicator, pastRunRowByline } from "@/components/agent-session-row"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
import { rowPrState, useAgentsData, usePastRuns, type AgentSessionRow } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import { agentLabel } from "@/components/agent-usage-bar"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// EXP-818: the Agent page's SHELL — the master-detail the Support page
// already has: the caller's sessions list on the left (Running, then Past —
// the two sections the Devices page carried until now), the selected
// session (or the chat prompt while nothing is selected) on the right. Both
// `/t/$teamSlug/agent` and `/t/$teamSlug/sessions/$sessionId` render inside
// it, so opening a run never loses the list. Below `md` the list is the page
// and a session is its own screen, as before.
//
// A run started by another run (`parent_session_id`) nests under its parent
// (`lib/session-tree.ts`, the ×4 rule), indented.

const ActionChatIcon = conceptIcon(`action-chat`)

export function AgentShell({
  teamId,
  currentUserId,
  activeSessionId,
  children,
}: {
  teamId: string
  currentUserId: string
  /** The session the right pane shows, for the highlighted row. */
  activeSessionId: string | null
  children: ReactNode
}) {
  return (
    <div className="flex h-full min-h-0">
      <div className="hidden w-80 shrink-0 flex-col border-r border-border md:flex">
        <SessionsList
          teamId={teamId}
          currentUserId={currentUserId}
          activeSessionId={activeSessionId}
        />
      </div>
      <div className="flex min-w-0 flex-1 flex-col">{children}</div>
    </div>
  )
}

/** The list alone — the phone's Agent page IS this list over the prompt. */
export function SessionsList({
  teamId,
  currentUserId,
  activeSessionId,
  className,
}: {
  teamId: string
  currentUserId: string
  activeSessionId: string | null
  className?: string
}) {
  const { running, isLoading } = useAgentsData(teamId, currentUserId)
  const { past } = usePastRuns(teamId, currentUserId)
  const openSession = useOpenSession()
  const runningById = new Map(running.map((row) => [row.session.id, row]))
  const tree = nestSessions(running.map((row) => row.session))
  return (
    <div className={cn(`flex-1 overflow-y-auto p-2`, TAB_BAR_CLEARANCE, className)}>
      <GlassSectionHeader label="Running" />
      {isLoading ? (
        <div className="px-3 py-2 text-xs text-muted-foreground">Loading…</div>
      ) : tree.length === 0 ? (
        <div className="px-3 py-2 text-xs text-muted-foreground">
          No agents running right now.
        </div>
      ) : (
        <div className="flex flex-col">
          {tree.map(({ session, depth }) => {
            const row = runningById.get(session.id)!
            return (
              <RunningRow
                key={session.id}
                row={row}
                depth={depth}
                active={session.id === activeSessionId}
                onOpen={() => openSession(session)}
              />
            )
          })}
        </div>
      )}
      {past.length > 0 && (
        <div className="mt-4">
          <GlassSectionHeader label="Past" />
          <div className="flex flex-col">
            {past.map((row) => (
              <PastRow
                key={row.session.id}
                session={row.session}
                title={row.title}
                identifier={row.identifier}
                byline={pastRunRowByline(row)}
                active={row.session.id === activeSessionId}
                onOpen={() => openSession(row.session)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/** One running row: the state dot, the identity, the host machine muted —
 * the sidebar's Sessions row and the IDE rail's, one more time. */
function RunningRow({
  row,
  depth,
  active,
  onOpen,
}: {
  row: AgentSessionRow
  depth: number
  active: boolean
  onOpen: () => void
}) {
  const { session, issue, device, paused } = row
  const identity = sessionIdentity(row)
  const state = sessionDisplayState(session, rowPrState(session, issue))
  const isChat = identity.identifier === null && session.actionName === `Chat`
  return (
    <ListRow
      interactive
      active={active}
      onClick={onOpen}
      className={cn(`gap-2 px-3 py-2`, paused && `opacity-60`)}
      style={{ paddingLeft: `${12 + depth * 14}px` }}
      data-testid={`agent-list-session-${issue?.identifier ?? session.id}`}
    >
      <span className="flex w-3 shrink-0 items-center justify-center">
        {isChat ? (
          <ActionChatIcon className="size-3.5 text-muted-foreground" />
        ) : (
          <RunningIndicator state={state} paused={paused} />
        )}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          {identity.identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {identity.identifier}
            </span>
          )}
          <span className="truncate">{identity.subject}</span>
        </div>
        <div className="truncate text-xs text-muted-foreground">
          {paused ? `Paused · ` : ``}
          {device.label || session.deviceLabel || `Desktop`}
          {` · started ${relativeTime(session.startedAt)}`}
        </div>
      </div>
    </ListRow>
  )
}

function PastRow({
  session,
  title,
  identifier,
  byline,
  active,
  onOpen,
}: {
  session: CodingSession
  title: string
  identifier: string | null
  byline: string
  active: boolean
  onOpen: () => void
}) {
  return (
    <ListRow
      interactive
      active={active}
      onClick={onOpen}
      className="gap-2 px-3 py-2"
      data-testid={`agent-list-session-${session.id}`}
    >
      <span className="flex w-3 shrink-0 items-center justify-center">
        <span className="inline-flex size-2 rounded-full bg-muted-foreground/40" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          {identifier && (
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {identifier}
            </span>
          )}
          <span className="truncate">{title}</span>
        </div>
        <div className="truncate text-xs text-muted-foreground">
          {byline || (session.agent ? agentLabel(session.agent) : ``)}
        </div>
      </div>
    </ListRow>
  )
}

/** The `$sessionId` the route is on, if any — the shell's highlight. */
export function useRouteSessionId(): string | null {
  const { sessionId } = useParams({ strict: false })
  return sessionId ?? null
}
