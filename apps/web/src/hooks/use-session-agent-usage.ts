import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { deviceCollection } from "@/lib/collections"
import type { CodingSession, Device, DeviceAgentUsage } from "@/db/schema"
import { useNow } from "@/hooks/use-now"
import { sessionAgentUsage } from "@/lib/agent-usage"

// EXP-484: the usage bar a session view draws — the host machine's FRESH
// report for the run's own agent, off the synced devices row. All the
// "should this render at all" rules (live status only, own agent only, fresh
// and non-empty) live in `sessionAgentUsage`, hand-mirrored ×4; this hook is
// just the live query plus the 30 s tick that ages a report out on its own
// (the use-session-device idiom).
export function useSessionAgentUsage(
  session: Pick<
    CodingSession,
    `deviceId` | `userId` | `agent` | `status`
  > | null
): { agent: string; usage: DeviceAgentUsage } | null {
  const deviceId = session?.deviceId ?? null
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      deviceId
        ? query
            .from({ d: deviceCollection })
            .where(({ d }) => eq(d.deviceId, deviceId))
        : undefined,
    [deviceId]
  )
  const now = useNow(30_000)
  const userId = session?.userId ?? null
  const agent = session?.agent ?? null
  const status = session?.status ?? null
  return useMemo(() => {
    if (userId === null || status === null) return null
    return sessionAgentUsage(
      { deviceId, userId, agent, status },
      (deviceRows ?? []) as Device[],
      now
    )
  }, [deviceId, userId, agent, status, deviceRows, now])
}
