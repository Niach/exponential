import { useMemo, useState } from "react"
import { useParams } from "@tanstack/react-router"
import { nestSessions, visibleTreeRows } from "@/lib/session-tree"
import { cn } from "@/lib/utils"
import { pastRunRowByline } from "@/components/agent-session-row"
import {
  PastSessionRow,
  RunningSessionRow,
} from "@/components/session-list-rows"
import { useSteerConfig } from "@/components/agent-session"
import { GlassSectionHeader } from "@/components/ui/glass-rows"
import { useAgentsData, usePastRuns } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { DetailOrigin } from "@/lib/detail-origin"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// EXP-818: the caller's sessions list — Running (nested by
// `parent_session_id`, the x4 rule) then Past. EXP-851 dissolved the
// master-detail shell it used to live in: the list is the Agent page's own
// column on every breakpoint, and the SIDEBAR's list nav renders the very
// same component beside an open run.

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
  // EXP-874: the rows' trailing buttons need the caller's role and whether a
  // conflicted Merge may launch "Fix conflicts" — resolved ONCE per list (the
  // permissions hook fetches the billing plan), never per row. The team comes
  // off the route: every mount lives under `/t/$teamSlug`.
  const { teamSlug } = useParams({ strict: false })
  const team = useTeamBySlug(teamSlug ?? ``)
  const { isMember, isOwner } = useTeamPermissions(
    team?.id === teamId ? team : null
  )
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
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
                  <RunningSessionRow
                    key={session.id}
                    row={row}
                    depth={depth}
                    active={session.id === activeSessionId}
                    expandable={hasChildren}
                    expanded={!collapsed.has(session.id)}
                    onToggle={() => toggle(session.id)}
                    isOwner={isOwner}
                    steerEnabled={steerEnabled}
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
                <PastSessionRow
                  key={row.session.id}
                  sessionId={row.session.id}
                  title={row.title}
                  identifier={row.identifier}
                  byline={pastRunRowByline(row)}
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
