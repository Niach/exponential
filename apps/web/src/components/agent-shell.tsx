import { useMemo, useState } from "react"
import type { CodingSession } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { nestSessions, visibleTreeRows } from "@/lib/session-tree"
import { sessionIdentity } from "@/lib/session-identity"
import { cn } from "@/lib/utils"
import { relativeTime } from "@/components/comment-rows/format"
import { RunningIndicator, pastRunRowByline } from "@/components/agent-session-row"
import {
  sessionAgentCaption,
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
import { rowPrState, useAgentsData, usePastRuns, type AgentSessionRow } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import type { DetailOrigin } from "@/lib/detail-origin"
import { agentLabel } from "@/components/agent-picker"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// EXP-818: the caller's sessions list — Running (nested by
// `parent_session_id`, the x4 rule) then Past. EXP-851 dissolved the
// master-detail shell it used to live in: the list is the Agent page's own
// column on every breakpoint, and the SIDEBAR's list nav renders the very
// same component beside an open run.

const ActionChatIcon = conceptIcon(`action-chat`)
// EXP-849: the same fold twisty the sidebar's Sessions group carries - the two
// lists render the same tree, so they fold the same way.
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

/** The list — the Agent page's own column, and the sidebar's Agent list nav.
 *  Every row hands the run it opens the `origin` this list stands for, so the
 *  nav stays put (EXP-851); the Agent page passes `{ kind: `agent` }`. */
export function SessionsList({
  teamId,
  currentUserId,
  activeSessionId,
  className,
  origin = null,
  scroll = true,
  showWhenEmpty = false,
}: {
  teamId: string
  currentUserId: string
  activeSessionId: string | null
  className?: string
  origin?: DetailOrigin | null
  /** EXP-851: the Agent page stacks the list UNDER the composer inside one
   *  scroller, so it turns this list's own scrollport off. */
  scroll?: boolean
  /** EXP-862: the Running band is ALWAYS drawn on the Agent page, empty or
   *  not — that page IS the composer plus what is running. The sidebar's
   *  17rem list nav leaves it off when nothing runs instead of parking a dead
   *  band there (desktop `RunningSessionsSection::show_when_empty`). */
  showWhenEmpty?: boolean
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
  // EXP-862: Past is FOLDED by default — the page is the composer plus what
  // is running; the history is one click away and says how much it holds
  // (the band's trailing count). Desktop `glass_section_band_fold` twin.
  const [pastOpen, setPastOpen] = useState(false)
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
    <div
      className={cn(
        `p-2`,
        scroll && `min-h-0 flex-1 overflow-y-auto`,
        TAB_BAR_CLEARANCE,
        className
      )}
    >
      {(showWhenEmpty || tree.length > 0) && (
        <>
          <GlassSectionHeader label="Running" />
          {isLoading ? (
            <div className="px-3 py-2 text-xs text-muted-foreground">
              Loading…
            </div>
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
                    onOpen={() => openSession(session, { origin })}
                  />
                )
              })}
            </div>
          )}
        </>
      )}
      {past.length > 0 && (
        <div className="mt-4">
          <GlassSectionHeader
            label="Past"
            count={past.length}
            expanded={pastOpen}
            onToggle={() => setPastOpen((open) => !open)}
          />
          {pastOpen && (
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
                  onOpen={() => openSession(row.session, { origin })}
                />
              ))}
            </div>
          )}
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
  const caption = sessionAgentCaption(session)
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
        {/* EXP-850 §8: the run's own caption (its live workflow), the SECOND
            line, before the device byline. */}
        {caption && (
          <div className="truncate text-xs text-muted-foreground" title={caption}>
            {caption}
          </div>
        )}
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
