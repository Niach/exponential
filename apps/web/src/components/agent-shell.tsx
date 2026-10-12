import { ThreadListSection, useOpenThread } from "@/components/thread/thread-list"
import { useThreadList, type ThreadBundle } from "@/hooks/use-threads"
import { cn } from "@/lib/utils"
import type { DetailOrigin } from "@/lib/detail-origin"
import type { Team } from "@/db/schema"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { groupByTeam, TeamBandHeader } from "@/components/team/team-band"

// EXP-818: the caller's list on the phone Agent page, under the composer.
//
// EXP-923: on md+ the whole list left the Agent page for the sidebar (the
// Running section in the main menu, the Recent panel behind the page's
// history button); the phone keeps it here.
//
// EXP-1284: lists show THREADS, not bare runs (draft 3): RUNNING = the
// threads holding a live run of mine (`partitionThreadList`), each row the
// thread's title over its list caption, opening the thread page. Recent is
// the home column's own band above it (`HomeRecent`, agent.tsx).

export function SessionsList({
  teamId,
  teams,
  grouped = false,
  currentUserId,
  activeSessionId: _activeSessionId,
  className,
  origin = null,
  scroll = true,
  showWhenEmpty = false,
}: {
  teamId: string
  /** EXP-1186: the teams the list reads (the phone: every member team);
   *  absent = `teamId` alone. A row opens under ITS team's slug. */
  teams?: readonly Team[]
  /** EXP-1186: one band per team instead of the one Running band. */
  grouped?: boolean
  currentUserId: string
  activeSessionId: string | null
  className?: string
  origin?: DetailOrigin | null
  /** EXP-851: the Agent page stacks the list UNDER the composer inside one
   *  scroller, so it turns this list's own scrollport off. */
  scroll?: boolean
  /** EXP-862: the Running band is ALWAYS drawn on the Agent page, empty or
   *  not — that page IS the composer plus what is running. */
  showWhenEmpty?: boolean
}) {
  const scopeIds = teams && teams.length > 0 ? teams.map((t) => t.id) : teamId
  const { running, isReady } = useThreadList(scopeIds, currentUserId)
  const openThread = useOpenThread()
  if (!showWhenEmpty && running.length === 0) return null
  const slugOf = (id: string) => teams?.find((team) => team.id === id)?.slug
  const open = (bundle: ThreadBundle) => {
    const teamSlug = slugOf(bundle.thread.teamId)
    openThread(bundle, { origin, ...(teamSlug ? { teamSlug } : {}) })
  }
  const groups =
    grouped && teams && isReady
      ? groupByTeam(teams, running, (bundle) => bundle.thread.teamId)
      : []
  return (
    <div
      className={cn(
        `p-2`,
        scroll && `min-h-0 flex-1 overflow-y-auto`,
        TAB_BAR_CLEARANCE,
        className
      )}
      data-testid="agent-thread-list"
    >
      {groups.length > 0 ? (
        // EXP-1186: one band per team, in team order — the team mark
        // leading, its running threads first.
        groups.map((group) => (
          <div key={group.team.id} className="mb-4 last:mb-0">
            <TeamBandHeader team={group.team} />
            <ThreadListSection threads={group.rows} onOpen={open} />
          </div>
        ))
      ) : (
        <ThreadListSection
          label="Running"
          threads={running}
          onOpen={open}
          loading={!isReady}
          empty="running-agents"
          testId="agent-running-threads"
        />
      )}
    </div>
  )
}
