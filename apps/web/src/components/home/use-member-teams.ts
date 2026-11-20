import { useMemo } from "react"
import { sidebarTeams } from "@exp/domain-contract/sidebar-tree"
import type { Team } from "@/db/schema"
import { useTeamMemberships } from "@/hooks/use-team-data"

// #EXP-2027 (sidebar-tree.json `teams`): the team-less pages read EVERY
// member team, in the sidebar's team order (by name, case-insensitive, ties
// by id; the personal team never shows).

/** The member teams in team order, pure. */
export function memberTeamsInOrder(teams: readonly Team[]): Team[] {
  const byId = new Map(teams.map((team) => [team.id, team]))
  return sidebarTeams(
    teams.map((team) => ({
      id: team.id,
      name: team.name,
      kind: (team as Team & { kind?: string | null }).kind ?? `team`,
    }))
  )
    .map((id) => byId.get(id))
    .filter((team): team is Team => team !== undefined)
}

export function useMemberTeams(me: string | null | undefined): {
  teams: Team[]
  /** Their ids in team order. */
  teamIds: string[]
  /** id → team. */
  byId: ReadonlyMap<string, Team>
} {
  const { myTeams } = useTeamMemberships(me ?? undefined)
  const signature = myTeams.map((team) => `${team.id}:${team.name}:${team.slug}:${team.iconUrl ?? ``}`).join(`|`)
  return useMemo(() => {
    const teams = memberTeamsInOrder(myTeams as Team[])
    return { teams, teamIds: teams.map((team) => team.id), byId: new Map(teams.map((team) => [team.id, team])) }
    // `myTeams` is a fresh array per render: key on its content.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature])
}
