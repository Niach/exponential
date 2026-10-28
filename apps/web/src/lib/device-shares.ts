import { eq, sql } from "drizzle-orm"
import { devices } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"
import { firstSharerTeams } from "@/lib/sessions/host"
import { foreignHostedSessionIds, pauseDeviceAgents } from "@/lib/sessions/lifecycle"

export type ShareTarget = { id: string; userId: string; deviceId: string }

/** The runs a withdrawn share hosted, per team. */
export type HostedByTeam = Array<{ teamId: string; ids: string[] }>

/** Every id of a `HostedByTeam`. */
export const hostedIds = (hosted: HostedByTeam): string[] => hosted.flatMap((entry) => entry.ids)

/**
 * FEED-149/FEED-154: withdraw `target`'s share of its machine from
 * `teamIds` for ANOTHER account (a team owner's `devices.dropShare`, an
 * instance admin's `admin.dropDeviceShare`, the key holder's register
 * clearing a squat). Same consequences as the owner's own unshare: the set
 * of runs it hosted for teammates is read here, under the machine's lock
 * (`lockDeviceShares`, the caller holds it) and before the write, for the
 * caller to end after commit; when it was the machine's first sharer the
 * team's agents on it are disarmed. Returns the hosted ids per team.
 */
export async function withdrawShareInTx(
  tx: Tx,
  target: ShareTarget,
  teamIds: readonly string[]
): Promise<HostedByTeam> {
  if (teamIds.length === 0) return []
  const ending: HostedByTeam = []
  for (const teamId of teamIds) {
    ending.push({ teamId, ids: await foreignHostedSessionIds(tx, target.userId, teamId, [target.deviceId]) })
  }
  const pause = await firstSharerTeams(tx, target, teamIds)
  await tx
    .update(devices)
    .set({
      sharedTeamIds: sql`ARRAY(SELECT unnest(${devices.sharedTeamIds}) EXCEPT SELECT unnest(${sql.param([...teamIds])}::uuid[]) ORDER BY 1)`,
      updatedAt: new Date(),
    })
    .where(eq(devices.id, target.id))
  if (pause.length > 0) {
    await pauseDeviceAgents(tx, pause, target.deviceId)
  }
  return ending
}
