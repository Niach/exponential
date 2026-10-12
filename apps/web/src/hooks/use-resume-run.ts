import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import type { CodingSession, Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import { deviceCanResumeRun, deviceRowIsOnline } from "@/lib/steer-devices"
import { useNow } from "@/hooks/use-now"
import { useResumeStart } from "@/hooks/use-resume-start"
import { useSessionDevice } from "@/hooks/use-session-device"

// EXP-877: the Resume machinery, lifted out of the session route's ended-run
// header (EXP-773) so the ONE coding action — the issue tray's and the run
// header's — can offer it from the same rule (`run-action-pills.tsx`).

/** EXP-637: Resume relaunches the run on the machine that still holds its
 * worktree — the caller hides the pill when that machine is offline or too
 * old to resume, rather than failing after the click. `null` = no candidate,
 * so the hook stays unconditional in a component that may have none. */
export function useCanResumeOn(
  session: Pick<CodingSession, `deviceId` | `userId`> | null
): boolean {
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const now = useNow(30_000)
  const deviceId = session?.deviceId ?? null
  const userId = session?.userId ?? null
  return useMemo(() => {
    if (!deviceId) return false
    const rows = ((deviceRows ?? []) as Device[]).filter(
      (row) => row.deviceId === deviceId
    )
    const row = rows.find((r) => r.userId === userId) ?? rows[0]
    if (!row) return false
    return (
      deviceRowIsOnline(row.lastSeenAt, now) &&
      deviceCanResumeRun({ caps: row.caps ?? [] })
    )
  }, [deviceRows, deviceId, userId, now])
}

/** EXP-818: a resume OPENS the run it started, exactly like a remote start
 * does (`use-remote-start.ts`): the relaunched run arrives as a NEW row over
 * Electric, and leaving the reader on the dead one (with a spinner that never
 * settles) was the whole complaint. `resumedFromId` names it. The command is
 * only half of it: `resuming` stays true until the new row syncs in and its
 * page opens, and says so if the machine never starts it (the desktop holds
 * the reason — a conflicted worktree, a failed doctor). EXP-1259: the send,
 * its startId and its Retries live in `useResumeStart`. */
export function useResumeRun(session: CodingSession): {
  resuming: boolean
  resume: () => Promise<void>
} {
  const device = useSessionDevice(session)
  const { pending, resume } = useResumeStart({
    sessionId: session.id,
    userId: session.userId,
    deviceId: session.deviceId,
    deadlineTitle: () =>
      `${device.label ?? `That machine`} never started this session`,
    failure: { verb: `resume`, thing: `the session` },
  })
  return {
    resuming: pending !== null,
    resume: async () => resume(),
  }
}
