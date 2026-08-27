// EXP-445: the unshare/deletion kill fan-out for sessions running on somebody
// ELSE's hardware. A teammate-started run on a shared server device (EXP-432)
// has `user_id` = requester and `host_user_id` = device owner, and nothing used
// to end it when the consent behind it disappeared — unsharing the box or
// deleting an account left the agent alive on the host's machine with no client
// able to reach it.
//
// Since EXP-560 every live row carries its `device_id` stamp, so the unshare
// path scopes the kill to the one machine whose share was withdrawn; member
// removal passes no deviceId and sweeps every box the host runs for them.
import { and, eq, inArray, ne } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { getSteerRelayConfig, relayPostKill } from "@/lib/steer"

/** Statuses that still have a live agent behind them (in_review stays
 * steerable while the PR awaits review). */
const LIVE_STATUSES = [`running`, `in_review`] as const

/**
 * End every live session hosted by `hostUserId` for a REQUESTER other than
 * themselves within `teamId` — the sessions a share was the consent for.
 * `deviceId` narrows the kill to one machine (the unshare path since
 * EXP-560 — every live row carries its device stamp, so unsharing one of
 * two same-team servers no longer over-kills the other box); member removal
 * stays device-agnostic (a removed member's runs on ANY of the host's boxes
 * must die). Returns the ended session ids.
 */
export async function endForeignHostedSessions(
  hostUserId: string,
  teamId: string,
  deviceId?: string
): Promise<string[]> {
  const endedSessionIds = await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId
    const ended = await tx
      .update(codingSessions)
      .set({
        status: `ended`,
        endedAt: new Date(),
        endedBy: `system`,
        updatedAt: new Date(),
      })
      .where(
        and(
          eq(codingSessions.hostUserId, hostUserId),
          ne(codingSessions.userId, hostUserId),
          eq(codingSessions.teamId, teamId),
          ...(deviceId ? [eq(codingSessions.deviceId, deviceId)] : []),
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
