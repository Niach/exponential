import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { sortSkills } from "@exp/domain-contract/skills"
import type { SyncedSkill } from "@/db/schema"
import { skillCollection } from "@/lib/collections"

// #EXP-1659: a team's SKILLS off the synced `skills` shape (body-less;
// `trpc.skills.get` is the only body path), sorted by name (skills.json
// `sortCases`). Read by Settings › Skills, the + menu › Skills and the agent
// page's Skills row.
export function useSkills(teamId: string | null | undefined): {
  skills: SyncedSkill[]
  isReady: boolean
} {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ s: skillCollection }).where(({ s }) => eq(s.teamId, teamId))
        : undefined,
    [teamId]
  )
  const skills = useMemo(() => sortSkills((data ?? []) as SyncedSkill[]), [data])
  return { skills, isReady: Boolean(teamId) && isReady }
}
