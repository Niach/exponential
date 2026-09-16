import { useMemo, useState } from "react"
import { nestSessions, visibleTreeRows } from "@/lib/session-tree"
import { cn } from "@/lib/utils"
import { RunningSessionRow } from "@/components/session-list-rows"
import { SessionTreeList } from "@/components/session-tree-list"
import { GlassSectionHeader } from "@exp/ui"
import { useAgentsData, usePastRuns } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import type { DetailOrigin } from "@/lib/detail-origin"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// EXP-818: the caller's sessions list — Running (nested by
// `parent_session_id`, the x4 rule) then Recent. EXP-851 dissolved the
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
  const runningById = new Map(running.map((row) => [row.session.id, row]))
  // EXP-849: a parent run's subtree folds away here too — an orchestrator with
  // six children used to push Recent off the list. Expanded by default, per
  // parent, for as long as the list is mounted (the sidebar's rule).
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  // EXP-862: Recent (EXP-886, was "Past") is FOLDED by default — the page is
  // the composer plus what is running; the history is one click away. It
  // shows no count (EXP-886): an issue's own "Runs" band holds the full list.
  // Desktop `glass_section_band_fold` twin.
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
            label="Recent"
            expanded={pastOpen}
            onToggle={() => setPastOpen((open) => !open)}
          />
          {pastOpen && (
            /* EXP-897: Recent nests too — `PAST_RUN_CAP` already applied to
               the ROWS above, so an unlisted parent simply leaves its child a
               root (session-tree rule 2). */
            <SessionTreeList
              rows={past}
              activeSessionId={activeSessionId}
              onOpen={(session) => openSession(session, { origin })}
            />
          )}
        </div>
      )}
    </div>
  )
}
