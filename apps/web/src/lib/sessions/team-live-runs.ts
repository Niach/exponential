import { isCodingSessionStale } from "@exp/db-schema/domain"

import type { CodingSession } from "@/db/schema"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { isLiveRun } from "@/lib/past-runs"

// EXP-1075: session lists are TEAM-scoped, so a run left behind in another
// team is invisible until you switch to it. The team picker carries the
// signal: the caller's OWN live runs, grouped by team, so a dot on the
// switcher (and on each foreign team's row) says "something of yours is
// alive over there". Liveness is `useMyLiveRuns`' rule exactly — live by
// status (or EXP-888's sweep end) AND a fresh heartbeat — so the dot and the
// Running section can never disagree about one run. ×4 lockstep.

/** The fields the grouping reads — structural so a caller holding a partial
 *  row (or a test fixture) still passes one. */
export type TeamLiveRunSession = Pick<
  CodingSession,
  `teamId` | `userId` | `status` | `endedBy` | `needsInput` | `updatedAt`
>

/** One team's live-run summary: how many of MY runs are alive there, and
 *  whether any of them is parked on a question (the amber tone). */
export type TeamLiveRuns = { count: number; needsInput: boolean }

/** The caller's live runs grouped by team. Teams with none are absent from
 *  the map, so `map.get(id)` is the whole "does this team have a dot?" test. */
export function liveRunsByTeam(
  sessions: readonly TeamLiveRunSession[],
  { me, now }: { me: string; now: Date }
): Map<string, TeamLiveRuns> {
  const byTeam = new Map<string, TeamLiveRuns>()
  for (const session of sessions) {
    if (session.userId !== me) continue
    if (!isLiveRun(session)) continue
    if (isCodingSessionStale(new Date(session.updatedAt), now)) continue
    const entry = byTeam.get(session.teamId) ?? { count: 0, needsInput: false }
    entry.count += 1
    // `prState` is not read here: the picker's dot only splits parked-on-me
    // from everything else, and an `in_review` run is never `needs_input`.
    if (sessionDisplayState(session, null) === `needs_input`) {
      entry.needsInput = true
    }
    byTeam.set(session.teamId, entry)
  }
  return byTeam
}

/** The switcher's own dot: the ACTIVE team is excluded — its runs are the
 *  sidebar's Running section, one floor below, and repeating them on the
 *  picker would say "go somewhere else" about work already on screen. */
export function otherTeamsLive(
  byTeam: ReadonlyMap<string, TeamLiveRuns>,
  activeTeamId: string | undefined
): { any: boolean; needsInput: boolean } {
  let any = false
  let needsInput = false
  for (const [teamId, entry] of byTeam) {
    if (teamId === activeTeamId) continue
    if (entry.count === 0) continue
    any = true
    if (entry.needsInput) needsInput = true
  }
  return { any, needsInput }
}
