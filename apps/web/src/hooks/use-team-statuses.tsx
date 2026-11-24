// EXP-314 — the React surface over lib/team-statuses.ts.
//
// `useTeamStatuses(teamId)` is the live-query hook; `TeamStatusesProvider`
// mounts it ONCE in the team layout so every row renderer (issue lists,
// pickers, filter pills, submenus) resolves through a plain context read
// instead of its own live query — a 200-row list must not open 200 queries.
import { createContext, useContext, useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { issueStatusCollection } from "@/lib/collections"
import type { IssueStatusRow } from "@/db/schema"
import {
  buildStatusOptions,
  defaultStatusOptions,
  resolveIssueStatus,
  type StatusResolvable,
  type StatusRowOption,
} from "@/lib/team-statuses"

export interface TeamStatuses {
  /** Team rows in display order; the constructed defaults while unsynced. */
  options: StatusRowOption[]
  byId: Map<string, StatusRowOption>
  resolve: (issue: StatusResolvable) => StatusRowOption
  /** False while `options` is still the constructed fallback set. */
  ready: boolean
}

function buildTeamStatuses(
  rows: readonly IssueStatusRow[] | undefined
): TeamStatuses {
  const ready = (rows?.length ?? 0) > 0
  const options = ready ? buildStatusOptions(rows!) : defaultStatusOptions()
  const byId = new Map(options.map((option) => [option.id, option]))
  return {
    options,
    byId,
    resolve: (issue) => resolveIssueStatus(issue, options, byId),
    ready,
  }
}

const FALLBACK_TEAM_STATUSES = buildTeamStatuses(undefined)

export function useTeamStatuses(teamId: string | undefined): TeamStatuses {
  const { data: rows } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ issueStatuses: issueStatusCollection })
            .where(({ issueStatuses }) => eq(issueStatuses.teamId, teamId))
        : undefined,
    [teamId]
  )

  return useMemo(
    () => buildTeamStatuses(rows as IssueStatusRow[] | undefined),
    [rows]
  )
}

/** #EXP-2079 (agent-settings.json `assistant.chatTeams` (6)): the statuses
 *  of SEVERAL teams, each issue resolved in its OWN team (`teamId` picks the
 *  set; an unknown team = the constructed defaults). */
export function useStatusesForTeams(teamIds: readonly string[]): (teamId: string | null | undefined) => TeamStatuses {
  const key = teamIds.join(`,`)
  const { data: rows } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query
            .from({ issueStatuses: issueStatusCollection })
            .where(({ issueStatuses }) => inArray(issueStatuses.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  return useMemo(() => {
    const byTeam = new Map<string, IssueStatusRow[]>()
    for (const row of (rows ?? []) as IssueStatusRow[]) {
      const list = byTeam.get(row.teamId)
      if (list) list.push(row)
      else byTeam.set(row.teamId, [row])
    }
    const built = new Map([...byTeam].map(([teamId, list]) => [teamId, buildTeamStatuses(list)]))
    return (teamId) => (teamId ? built.get(teamId) : undefined) ?? FALLBACK_TEAM_STATUSES
  }, [rows])
}

const TeamStatusesContext = createContext<TeamStatuses | null>(null)

export function TeamStatusesProvider({
  teamId,
  children,
}: {
  teamId: string | undefined
  children: React.ReactNode
}) {
  const value = useTeamStatuses(teamId)
  return (
    <TeamStatusesContext.Provider value={value}>
      {children}
    </TeamStatusesContext.Provider>
  )
}

/**
 * Status resolution for anything rendered under the team layout. Outside a
 * provider (tests, stray surfaces) it degrades to the constructed default set
 * — rendering never fails, it just can't show custom statuses.
 */
export function useTeamStatusesContext(): TeamStatuses {
  return useContext(TeamStatusesContext) ?? FALLBACK_TEAM_STATUSES
}
