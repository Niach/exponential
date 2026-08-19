// EXP-445: the unshare/deletion kill fan-out for sessions running on somebody
// ELSE's hardware. A teammate-started run on a shared server device (EXP-432)
// has `user_id` = requester and `host_user_id` = device owner, and nothing used
// to end it when the consent behind it disappeared — unsharing the box or
// deleting an account left the agent alive on the host's machine with no client
// able to reach it.
//
// The predicate deliberately carries NO device identity: rows written by
// pre-EXP-549 clients carry only a `device_label` SNAPSHOT (`device_id` is
// NULL there), and an under-kill would recreate exactly the lifecycle gap this
// closes. So an owner who shares TWO servers with the SAME team over-kills the
// other box's sessions when unsharing one — rare, and recoverable by restarting
// on the still-shared device.
import { and, eq, inArray, ne } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { getSteerRelayConfig, relayPostKill } from "@/lib/steer"

/** Statuses that still have a live agent behind them (EXP-358 keeps
 * in_review/merged steerable). */
const LIVE_STATUSES = [`running`, `in_review`, `merged`] as const

/**
 * End every live session hosted by `hostUserId` for a REQUESTER other than
 * themselves within `teamId` — the sessions a share was the consent for.
 * Returns the ended session ids.
 */
export async function endForeignHostedSessions(
  hostUserId: string,
  teamId: string
): Promise<string[]> {
  const endedSessionIds = await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId
    const ended = await tx
      .update(codingSessions)
      .set({ status: `ended`, endedAt: new Date(), updatedAt: new Date() })
      .where(
        and(
          eq(codingSessions.hostUserId, hostUserId),
          ne(codingSessions.userId, hostUserId),
          eq(codingSessions.teamId, teamId),
          inArray(codingSessions.status, [...LIVE_STATUSES])
        )
      )
      .returning({ id: codingSessions.id })
    return ended.map((s) => s.id)
  })

  await relayKillSessionsBestEffort(endedSessionIds)
  return endedSessionIds
}

/**
 * Best-effort relay kills for already-ended rows — the durable abort path is
 * the committed `ended` flip (which the desktop reads off Electric and the CLI
 * daemon off its kill-poll); this only makes the teardown immediate.
 * `relayPostKill` never throws, and an unconfigured relay is a no-op.
 */
export async function relayKillSessionsBestEffort(
  sessionIds: string[]
): Promise<void> {
  if (sessionIds.length === 0) return
  const config = getSteerRelayConfig()
  if (!config) return
  await Promise.all(sessionIds.map((id) => relayPostKill(config, id)))
}
