// EXP-909 (EXP-881 §B5): the run's usage overlay asks for FRESH numbers the
// moment it opens — one `agent_usage_refresh` on the run's own machine, for the
// run's own login, and then nothing. No polling loop while it is up: the
// device's next beat (~30 s) delivers the answer through Electric, and the
// overlay dims + captions whatever it has in the meantime (`usageAge`).
//
// The same command the Devices page queues, under the same floors: only on one
// of the caller's OWN machines and only for the caller's OWN run, only while
// the machine is online and advertises `agent-usage-refresh`, only for a login
// that could answer at all (`usageRefreshEligible`), and never inside the
// device's own rate-limit floor (`refreshAllowedAt`) — a glance must not be
// able to out-poll what the machine allows itself.
//
// EXP-875: "one per open" is remembered PER RUN, module-level and briefly
// (`FIRED_TTL_MS`), not in a ref this hook's own mount owns: on a phone the
// overlay lives on a face that unmounts when the reader swaps to Changes and
// back, and a ref would have made every swap a fresh "first open". It fails
// QUIETLY in any case — `agent_usage_refresh` is an IDEMPOTENT command kind
// (`lib/trpc/devices.ts`), so a command still queued from a previous open is
// REUSED rather than refused, and a genuine failure is nothing a reader can
// act on. Nothing goes out at all while the tab is hidden.
//
// ×4: iOS/Android call `devices.createCommand` the same way from their sheets;
// a DESKTOP run refreshes locally instead (`device_sync::refresh_agent_usage_here`).
import { useEffect } from "react"
import { useLiveQuery, eq } from "@tanstack/react-db"
import type { contract } from "@exp/domain-contract"
import type { CodingSession, Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { refreshAllowedAt, type AgentProfileUsageRow } from "@/lib/agent-usage"
import { deviceCanRefreshUsage, deviceRowIsOnline } from "@/lib/steer-devices"
import {
  pageIsVisible,
  usageRefreshEligible,
} from "@/hooks/use-agent-usage-refresh"

/** How long one run's "already asked on open" verdict stands. Matched to the
 * device's own rate-limit floor: past it the floor is the guard again. */
export const FIRED_TTL_MS = 5 * 60 * 1000

/** Run id + login → when this page last asked on an open. Module-level so a
 * face swap (or any remount of the overlay's owner) is not a new first open. */
const firedAt = new Map<string, number>()

/** Test seam. */
export function resetSessionUsageRefresh(): void {
  firedAt.clear()
}

function claimFirstOpen(key: string, at: number): boolean {
  for (const [entry, stamp] of firedAt) {
    if (at - stamp > FIRED_TTL_MS) firedAt.delete(entry)
  }
  if (firedAt.has(key)) return false
  firedAt.set(key, at)
  return true
}

export function useSessionUsageRefreshOnOpen(
  open: boolean,
  session: Pick<CodingSession, `id` | `deviceId` | `userId` | `agent`> | null,
  /** The login the overlay is showing — the RUN's account (EXP-909 §1), not
   *  the machine's active one. Null while it is unknown: there is no login to
   *  name in the command, so nothing is queued. */
  row: Pick<
    AgentProfileUsageRow,
    `profileId` | `usage` | `mine` | `signedIn` | `unmonitored`
  > | null,
  currentUserId: string
): void {
  const deviceId = session?.deviceId ?? null
  const agent = session?.agent ?? null
  const sessionId = session?.id ?? null
  // EXP-875: a teammate's run is read-only here — its machine takes no
  // commands from this reader, and the row's `mine` says the same thing about
  // the login.
  const ownRun = session?.userId === currentUserId
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
  const eligible = row !== null && usageRefreshEligible(row)
  useEffect(() => {
    if (!open || !pageIsVisible()) return
    if (!sessionId || !deviceId || !agent || !profileId) return
    if (!ownRun || !eligible) return
    const device = ((deviceRows ?? []) as Device[]).find(
      (candidate) => candidate.userId === currentUserId
    )
    if (!device) return
    const now = new Date()
    if (!deviceRowIsOnline(device.lastSeenAt, now)) return
    if (!deviceCanRefreshUsage({ caps: device.caps ?? [] })) return
    if (refreshAllowedAt(usage, now) !== null) return
    if (!claimFirstOpen(`${sessionId}:${profileId}`, now.getTime())) return
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
  }, [
    open,
    sessionId,
    deviceId,
    agent,
    profileId,
    ownRun,
    eligible,
    usage,
    deviceRows,
    currentUserId,
  ])
}
