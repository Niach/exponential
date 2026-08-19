import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { deviceCollection } from "@/lib/collections"
import type { CodingSession, Device } from "@/db/schema"
import { useNow } from "@/hooks/use-now"
import { resolveSessionDevice, type SessionDevice } from "@/lib/session-device"

// EXP-549/550: a session's host machine as the synced devices row knows it —
// the RENAMED label and live online-ness — for components that hold only the
// session row (the session view, issue-detail coding rows). List surfaces
// get the same off `useAgentsData` rows. Ticks every 30 s against the 90 s
// online window (the use-remote-start idiom).
export function useSessionDevice(
  session: Pick<CodingSession, `deviceId` | `deviceLabel` | `userId`> | null
): SessionDevice {
  const enabled = session !== null
  const { data: deviceRows } = useLiveQuery(
    (query) => (enabled ? query.from({ d: deviceCollection }) : undefined),
    [enabled]
  )
  const now = useNow(30_000)
  const deviceId = session?.deviceId ?? null
  const deviceLabel = session?.deviceLabel ?? null
  const userId = session?.userId ?? null
  return useMemo(
    () =>
      userId === null
        ? { label: deviceLabel, online: null }
        : resolveSessionDevice(
            { deviceId, deviceLabel, userId },
            (deviceRows ?? []) as Device[],
            now
          ),
    [deviceId, deviceLabel, userId, deviceRows, now]
  )
}
