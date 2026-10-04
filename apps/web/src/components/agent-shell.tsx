import { GlassSectionHeader } from "@exp/ui"
import { cn } from "@/lib/utils"
import { SessionTree } from "@/components/session-tree"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import type { DetailOrigin } from "@/lib/detail-origin"
import type { CodingSession, Team } from "@/db/schema"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { groupByTeam, TeamBandHeader } from "@/components/team/team-band"

// EXP-818: the caller's sessions list — Running, nested by
// `parent_session_id` (the ×4 rule) with the EXP-965 connector.
//
// EXP-996: and the nesting is the whole `sessionTree` now, resumes collapsed.
//
// EXP-923: RUNNING ONLY. The Recent band is gone from here: on md+ the whole
// list left the Agent page for the sidebar (the Running section in the main
// menu, the Recent panel behind the page's history button), and on the phone
// Recent moved into the topbar's history sheet. What is left is the phone
// Agent page's own Running list under the composer.

/** The Running list — the Agent page's column on the phone. Every row hands
 *  the run it opens the `origin` this list stands for (EXP-851). */
export function SessionsList({
  teamId,
  teams,
  grouped = false,
  currentUserId,
  activeSessionId,
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
  const { running, isLoading } = useAgentsData(scopeIds, currentUserId)
  const openSession = useOpenSession()
  if (!showWhenEmpty && running.length === 0) return null
  const slugOf = (id: string) => teams?.find((team) => team.id === id)?.slug
  const open = (session: CodingSession) => {
    const teamSlug = slugOf(session.teamId)
    openSession(session, { origin, ...(teamSlug ? { teamSlug } : {}) })
  }
  const groups =
    grouped && teams && !isLoading
      ? groupByTeam(teams, running, (row) => row.session.teamId)
      : []
  return (
    <div
      className={cn(
        `p-2`,
        scroll && `min-h-0 flex-1 overflow-y-auto`,
        TAB_BAR_CLEARANCE,
        className
      )}
    >
      {groups.length > 0 ? (
        // EXP-1186: one band per team, in team order — the team mark
        // leading, its own runs nested under it.
        groups.map((group) => (
          <div key={group.team.id} className="mb-4 last:mb-0">
            <TeamBandHeader team={group.team} />
            <SessionTree
              rows={group.rows}
              activeSessionId={activeSessionId}
              onOpen={open}
            />
          </div>
        ))
      ) : (
        <>
          <GlassSectionHeader label="Running" />
          {isLoading ? (
            <div className="px-3 py-2 text-xs text-muted-foreground">
              Loading…
            </div>
          ) : (
            <SessionTree
              rows={running}
              activeSessionId={activeSessionId}
              onOpen={open}
              emptyNote="No agents running right now."
            />
          )}
        </>
      )}
    </div>
  )
}
