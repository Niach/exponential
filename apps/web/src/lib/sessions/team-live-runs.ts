import { isPersistent } from "@exp/domain-contract/session-kind"
import type { SessionRow } from "@/lib/session-tree"

// EXP-1075: session lists are TEAM-scoped, so a session left behind in
// another team is invisible until you switch to it. The team picker carries
// the signal: the caller's OWN live sessions (EXP-1293: `sessions` rows that
// have not ended), grouped by team, so a dot on the switcher (and on each
// foreign team's row) says "something of yours is alive over there".

/** The fields the grouping reads — structural so a caller holding a partial
 *  row (or a test fixture) still passes one. */
export type TeamLiveRunSession = Pick<
  SessionRow,
  `teamId` | `userId` | `status` | `needsInput` | `agentId` | `rootId`
>

/** A live session counts only when its ROOT is a thread: an agent's
 *  conversation session (NEW-2: a root with an `agentId`) never ends, so it
 *  and every session under it (`agentRootIds`) stay out of the "something
 *  runs" dots. */
export function countsAsLiveSession(
  session: Pick<SessionRow, `agentId` | `rootId` | `status`>,
  agentRootIds: ReadonlySet<string>
): boolean {
  if (session.status === `ended`) return false
  if (isPersistent(session)) return false
  if (session.rootId && agentRootIds.has(session.rootId)) return false
  return true
}

/** One team's live summary: how many of MY sessions are alive there, and
 *  whether any of them is parked on a question (the amber tone). */
export type TeamLiveRuns = { count: number; needsInput: boolean }

/** The caller's live sessions grouped by team. Teams with none are absent
 *  from the map, so `map.get(id)` is the whole "does this team have a dot?"
 *  test. */
export function liveRunsByTeam(
  sessions: readonly TeamLiveRunSession[],
  { me, agentRootIds = new Set() }: { me: string; agentRootIds?: ReadonlySet<string> }
): Map<string, TeamLiveRuns> {
  const byTeam = new Map<string, TeamLiveRuns>()
  for (const session of sessions) {
    if (session.userId !== me) continue
    if (!countsAsLiveSession(session, agentRootIds)) continue
    const entry = byTeam.get(session.teamId) ?? { count: 0, needsInput: false }
    entry.count += 1
    if (session.needsInput) entry.needsInput = true
    byTeam.set(session.teamId, entry)
  }
  return byTeam
}

/** The switcher's own dot: the ACTIVE team is excluded — its sessions are
 *  the sidebar's Running section, and repeating them on the picker would
 *  say "go somewhere else" about work already on screen. */
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
