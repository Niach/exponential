import { useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { SyncedPrompt } from "@/db/schema"
import { promptCollection } from "@/lib/collections"

// NEW-2: a team's saved PROMPTS off the synced `prompts` shape (body-less;
// `trpc.prompts.get` is the only body path), in the team's order. Read by
// Settings → Prompts and the composers' `/` menu.

/** Sort order first, then name: the server's `prompts.list` order. */
export function sortPrompts<T extends Pick<SyncedPrompt, `sortOrder` | `name`>>(
  rows: readonly T[]
): T[] {
  return [...rows].sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
}

export function usePrompts(teamId: string | null | undefined): {
  prompts: SyncedPrompt[]
  isReady: boolean
} {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ p: promptCollection }).where(({ p }) => eq(p.teamId, teamId))
        : undefined,
    [teamId]
  )
  const prompts = useMemo(() => sortPrompts((data ?? []) as SyncedPrompt[]), [data])
  return { prompts, isReady: Boolean(teamId) && isReady }
}

/** #EXP-2079 (agent-settings.json `assistant.chatTeams` (4)): the prompts of
 *  SEVERAL teams, team by team in `teamIds`' order, each team's in its own
 *  order. One team = exactly `usePrompts`. */
export function usePromptsForTeams(teamIds: readonly string[]): {
  prompts: SyncedPrompt[]
  isReady: boolean
} {
  const key = teamIds.join(`,`)
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query.from({ p: promptCollection }).where(({ p }) => inArray(p.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  const prompts = useMemo(() => {
    const rows = (data ?? []) as SyncedPrompt[]
    return key.split(`,`).flatMap((teamId) => sortPrompts(rows.filter((row) => row.teamId === teamId)))
  }, [data, key])
  return { prompts, isReady: teamIds.length > 0 && isReady }
}
