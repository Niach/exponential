import type { ReactNode } from "react"
import { GlassSectionHeader, TeamAvatar } from "@exp/ui"
import type { Team } from "@/db/schema"

// EXP-1186: a phone's cross-team lists (Agent Running + Recent, Actions)
// band their rows one TEAM at a time once the person is in more than one
// team — the existing group band with the team mark leading. ×3 with iOS
// and Android.
export function TeamBandHeader({
  team,
  trailing,
}: {
  team: Pick<Team, `id` | `name`>
  trailing?: ReactNode
}) {
  return (
    <div data-testid={`team-band-${team.id}`}>
      <GlassSectionHeader
        leading={<TeamAvatar name={team.name} size={16} />}
        label={team.name}
        trailing={trailing}
      />
    </div>
  )
}

/** Rows split per team in the given team order; teams with none dropped. */
export function groupByTeam<T>(
  teams: readonly Team[],
  rows: readonly T[],
  teamIdOf: (row: T) => string
): { team: Team; rows: T[] }[] {
  return teams
    .map((team) => ({
      team,
      rows: rows.filter((row) => teamIdOf(row) === team.id),
    }))
    .filter((group) => group.rows.length > 0)
}
