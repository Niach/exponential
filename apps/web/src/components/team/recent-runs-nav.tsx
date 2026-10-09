import { useParams } from "@tanstack/react-router"
import { GlassSectionHeader } from "@exp/ui"
import { SessionTree } from "@/components/session-tree"
import { usePastRuns } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import type { CodingSession, Team } from "@/db/schema"
import { groupByTeam, TeamBandHeader } from "@/components/team/team-band"

// EXP-923: the Agent page's RECENT runs. The page is the composer alone; its
// history sits behind ONE ghost history button that slides this list into the
// sidebar's panel slot (the phone gets the same list in a bottom sheet from
// the topbar instead).
//
// EXP-1246: Recent is a LIST-DETAIL list like the Inbox — the list-detail host
// (`list-detail.tsx`) renders it and a row opens its run with the
// `agent:recent` origin, so the list STAYS beside the run (the open one
// highlighted) and Back lands on the Agent page with it still open. Rows are
// the big `SessionRow` (title + caption), nested like every session list.

/** The md+ panel body — the host owns the back row. */
export function RecentRunsNav({
  teamId,
  currentUserId,
}: {
  teamId: string
  currentUserId: string | undefined
}) {
  const { past } = usePastRuns(teamId, currentUserId)
  const openSession = useOpenSession()
  const { sessionId } = useParams({ strict: false }) as { sessionId?: string }
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
      <GlassSectionHeader label="Recent" />
      <SessionTree
        size="big"
        ringClassName="ring-sidebar"
        rows={past}
        activeSessionId={sessionId ?? null}
        onOpen={(session) =>
          openSession(session, { origin: { kind: `agent`, tab: `recent` } })
        }
        emptyNote="Nothing has finished yet."
      />
    </div>
  )
}

/** The same rows for the phone's bottom sheet (`mobile-topbar.tsx`).
 *  EXP-1186: across every member team (`teams`), one band per team while
 *  `grouped`; a row opens under ITS team's slug. */
export function RecentRunsList({
  teamId,
  teams,
  grouped = false,
  currentUserId,
  onOpened,
}: {
  teamId: string
  teams?: readonly Team[]
  grouped?: boolean
  currentUserId: string | undefined
  onOpened: () => void
}) {
  const scopeIds = teams && teams.length > 0 ? teams.map((t) => t.id) : teamId
  const { past } = usePastRuns(scopeIds, currentUserId)
  const openSession = useOpenSession()
  const open = (session: CodingSession) => {
    onOpened()
    const teamSlug = teams?.find((team) => team.id === session.teamId)?.slug
    openSession(session, {
      origin: { kind: `agent` },
      ...(teamSlug ? { teamSlug } : {}),
    })
  }
  const groups =
    grouped && teams ? groupByTeam(teams, past, (row) => row.session.teamId) : []
  if (groups.length > 0) {
    return (
      <>
        {groups.map((group) => (
          <div key={group.team.id} className="mb-4 last:mb-0">
            <TeamBandHeader team={group.team} />
            <SessionTree rows={group.rows} onOpen={open} />
          </div>
        ))}
      </>
    )
  }
  return (
    <SessionTree
      rows={past}
      onOpen={open}
      emptyNote="Nothing has finished yet."
    />
  )
}
