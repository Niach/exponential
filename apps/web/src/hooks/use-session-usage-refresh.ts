// EXP-909 (EXP-881 §B5): the run's usage overlay asks for FRESH numbers the
// moment it opens — one `agent_usage_refresh` on the run's own machine, for the
// run's own login, and then nothing. No polling loop while it is up: the
// device's next beat (~30 s) delivers the answer through Electric, and the
// overlay dims + captions whatever it has in the meantime (`usageAge`).
//
// The same command the Devices page queues, under the same floors: only on one
// of the caller's OWN machines, only while it is online and advertises
// `agent-usage-refresh`, and never inside the device's own rate-limit floor
// (`refreshAllowedAt`) — a glance must not be able to out-poll what the machine
// allows itself. It fails QUIETLY: a command still queued from a previous open
// answers CONFLICT, and there is nothing for a reader to do about that.
//
// ×4: iOS/Android call `devices.createCommand` the same way from their sheets;
// a DESKTOP run refreshes locally instead (`device_sync::refresh_agent_usage_here`).
import { useEffect, useRef } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import type { contract } from "@exp/domain-contract"
import type { CodingSession, Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  refreshAllowedAt,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import { deviceCanRefreshUsage, deviceRowIsOnline } from "@/lib/steer-devices"

export function useSessionUsageRefreshOnOpen(
  open: boolean,
  session: Pick<CodingSession, `deviceId` | `userId` | `agent`> | null,
  /** The login the overlay is showing — the RUN's account (EXP-909 §1), not
   *  the machine's active one. Null while it is unknown: there is no login to
   *  name in the command, so nothing is queued. */
  row: Pick<AgentProfileUsageRow, `profileId` | `usage` | `mine`> | null,
  currentUserId: string
): void {
  const deviceId = session?.deviceId ?? null
  const agent = session?.agent ?? null
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      deviceId
        ? query
            .from({ d: deviceCollection })
            .where(({ d }) => eq(d.deviceId, deviceId))
        : undefined,
    [deviceId]
  )
  const profileId = row?.profileId ?? null
  const usage = row?.usage ?? null
  const mine = row?.mine ?? false
  // One request per OPEN: the flag clears when the overlay closes.
  const fired = useRef(false)
  useEffect(() => {
    if (!open) {
      fired.current = false
      return
    }
    if (fired.current) return
    if (!deviceId || !agent || !profileId || !mine) return
    const device = ((deviceRows ?? []) as Device[]).find(
      (candidate) => candidate.userId === currentUserId
    )
    if (!device) return
    const now = new Date()
    if (!deviceRowIsOnline(device.lastSeenAt, now)) return
    if (!deviceCanRefreshUsage({ caps: device.caps ?? [] })) return
    if (refreshAllowedAt(usage, now) !== null) return
    fired.current = true
    void trpc.devices.createCommand
      .mutate(
        {
          deviceId,
          kind: `agent_usage_refresh`,
          agent: agent as (typeof contract.codingAgent.values)[number],
          profileId,
        },
        { context: { skipErrorToast: true } }
      )
      .catch(() => {
        // Quietly: the numbers on screen keep their honest "as of …".
      })
  }, [open, deviceId, agent, profileId, mine, usage, deviceRows, currentUserId])
}
