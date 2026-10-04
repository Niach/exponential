import { GlassSectionHeader } from "@exp/ui"
import { SessionTree } from "@/components/session-tree"
import { usePastRuns } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"
import { setRecentRunsPanelOpen } from "@/lib/recent-runs-panel"
import type { CodingSession, Team } from "@/db/schema"
import { groupByTeam, TeamBandHeader } from "@/components/team/team-band"

// EXP-923: the Agent page's RECENT runs, as a sidebar panel.
//
// The Agent page is the composer and nothing else now — its history lives
// behind ONE ghost history button at the page's top-left, which slides this
// panel into the sidebar's panel slot beside the compact rail (the phone gets
// the same list in a bottom sheet from the topbar instead). Plain list, no
// fold: the band is a label, not a control — the toggle IS the disclosure.
//
// A row opens its run exactly as before, with the `agent` origin, so Back
// returns to the Agent page and a tab is created like any other opened work.
// EXP-1119: the panel wears the back row every slid-in panel does; it shuts
// the panel (the page's history button hides while it is up).

export function RecentRunsSidebar({
  teamId,
  currentUserId,
}: {
  teamId: string
  currentUserId: string | undefined
}) {
  const { past } = usePastRuns(teamId, currentUserId)
  const openSession = useOpenSession()
  return (
    <>
      <SidebarBackRow
        label="Agent"
        onBack={() => {
          setRecentRunsPanelOpen(false)
          // The panel (and this row) unmounts: hand focus back to the page's
          // history button so a keyboard user is not dropped on <body>.
          requestAnimationFrame(() =>
            document
              .querySelector<HTMLElement>(`[data-testid="recent-runs-toggle"]`)
              ?.focus(),
          )
        }}
      />
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
        <GlassSectionHeader label="Recent" />
        <SessionTree
          rows={past}
          onOpen={(session) =>
            openSession(session, { origin: { kind: `agent` } })
          }
          emptyNote="Nothing has finished yet."
        />
      </div>
    </>
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
