import { useMemo, useState, type ReactNode } from "react"
import { useParams } from "@tanstack/react-router"
import type { CodingSession } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { nestSessions, visibleTreeRows } from "@/lib/session-tree"
import { sessionIdentity } from "@/lib/session-identity"
import { cn } from "@/lib/utils"
import { relativeTime } from "@/components/comment-rows/format"
import { RunningIndicator, pastRunRowByline } from "@/components/agent-session-row"
import {
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"
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
// EXP-849: the same fold twisty the sidebar's Sessions group carries — the two
// lists render the same tree, so they fold the same way.
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

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
  // EXP-849: a parent run's subtree folds away here too — an orchestrator with
  // six children used to push Past off the list. Expanded by default, per
  // parent, for as long as the list is mounted (the sidebar's rule).
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  const nested = useMemo(
    () => nestSessions(running.map((row) => row.session)),
    [running]
  )
  const tree = useMemo(
    () => visibleTreeRows(nested, collapsed),
    [nested, collapsed]
  )
  // EXP-849: the continuation chain (`resumed_from_id`) — a run that was
  // switched onto another account or resumed says so, and the run it came out
  // of is marked as continued instead of looking like a second dead run.
  const continuedIds = useMemo(() => {
    const ids = new Set<string>()
    for (const row of [...running, ...past]) {
      if (row.session.resumedFromId) ids.add(row.session.resumedFromId)
    }
    return ids
  }, [running, past])
  const toggle = (sessionId: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(sessionId)) next.add(sessionId)
      return next
    })
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
          {tree.map(({ session, depth, hasChildren }) => {
            const row = runningById.get(session.id)!
            return (
              <RunningRow
                key={session.id}
                row={row}
                depth={depth}
                active={session.id === activeSessionId}
                expandable={hasChildren}
                expanded={!collapsed.has(session.id)}
                onToggle={() => toggle(session.id)}
                continuation={Boolean(session.resumedFromId)}
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
                continued={continuedIds.has(row.session.id)}
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
  expandable,
  expanded,
  onToggle,
  continuation,
  onOpen,
}: {
  row: AgentSessionRow
  depth: number
  active: boolean
  /** EXP-849: this run started others — the row carries the fold twisty. */
  expandable: boolean
  expanded: boolean
  onToggle: () => void
  /** EXP-849: this run took over from an earlier one (an account switch or a
   *  resume) — the byline says so. */
  continuation: boolean
  onOpen: () => void
}) {
  const { session, issue, device, paused } = row
  const identity = sessionIdentity(row)
  const prState = rowPrState(session, issue)
  const state = sessionDisplayState(session, prState)
  const working = sessionRowIsWorking(session, prState)
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
      {expandable ? (
        /* The twisty is its own target inside the row — a click folds the
           subtree instead of opening the run (the sidebar's rule). */
        <span
          role="button"
          tabIndex={-1}
          aria-label={expanded ? `Collapse child runs` : `Expand child runs`}
          className="flex w-3 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
          onClick={(event) => {
            event.stopPropagation()
            onToggle()
          }}
        >
          {expanded ? (
            <ChevronDownIcon className="size-3" />
          ) : (
            <ChevronRightIcon className="size-3" />
          )}
        </span>
      ) : (
        <span aria-hidden className="w-3 shrink-0" />
      )}
      <span className="flex w-3 shrink-0 items-center justify-center">
        {isChat ? (
          <ActionChatIcon className="size-3.5 text-muted-foreground" />
        ) : (
          <RunningIndicator state={state} paused={paused} working={working} />
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
          {continuation ? ` · continued` : ``}
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
  continued,
  active,
  onOpen,
}: {
  session: CodingSession
  title: string
  identifier: string | null
  byline: string
  /** EXP-849: another run took over from this one — it ended ON PURPOSE. */
  continued: boolean
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
          {continued ? ` · continued in a newer run` : ``}
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
