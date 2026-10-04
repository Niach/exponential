import { useMemo } from "react"
import { useIsMobile } from "@exp/ui"
import type { Team } from "@/db/schema"
import { useSession } from "@/hooks/use-session"
import { useTeamMemberships } from "@/hooks/use-team-data"

// EXP-1186: on a PHONE only the Issues tab is team-scoped (it has the board
// picker). Reviews, the Agent page's Running + Recent runs, Actions and the
// tab bar's dots read EVERY member team, like the inbox — ×3 with iOS and
// Android. md+ web keeps the active team (the sidebar's team picker scopes
// it). Lists group one band per team ONLY while the person is in more than
// one team, so a single-team phone stays pixel-identical to before.

/** The canonical team order the per-team bands follow: name, then id. */
export function compareTeams(
  left: Pick<Team, `id` | `name`>,
  right: Pick<Team, `id` | `name`>
): number {
  const byName = left.name.localeCompare(right.name)
  if (byName !== 0) return byName
  return left.id < right.id ? -1 : left.id > right.id ? 1 : 0
}

export interface CrossTeamScope {
  /** The teams the surface reads: every member team on a phone, else the
   *  active one. Ordered by `compareTeams`. */
  teams: Team[]
  /** Their ids, sorted — a stable live-query dependency. */
  teamIds: string[]
  /** Group the list one band per team (phone AND more than one team). */
  grouped: boolean
}

/** Pure: what a surface reads for the active team + the member teams. */
export function crossTeamScope(
  activeTeam: Team | null | undefined,
  memberTeams: readonly Team[],
  phone: boolean
): CrossTeamScope {
  // Until the memberships land (or off the phone) the active team alone.
  const teams =
    phone && memberTeams.length > 0
      ? [...memberTeams].sort(compareTeams)
      : activeTeam
        ? [activeTeam]
        : []
  const teamIds = teams.map((team) => team.id).sort()
  return { teams, teamIds, grouped: phone && teams.length > 1 }
}

export function useCrossTeamScope(
  activeTeam: Team | null | undefined
): CrossTeamScope {
  const phone = useIsMobile()
  const { data: session } = useSession()
  const { myTeams } = useTeamMemberships(phone ? session?.user?.id : undefined)
  const signature = myTeams
    .map((team) => `${team.id}:${team.name}:${team.slug}:${team.yoloMode}`)
    .join(`|`)
  return useMemo(
    () => crossTeamScope(activeTeam, myTeams as Team[], phone),
    // `myTeams` is a fresh array per render — key on its content.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [activeTeam, signature, phone]
  )
}
