import { useEffect, useMemo, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { toast } from "sonner"
import type { CodingSession, Device } from "@/db/schema"
import { codingSessionCollection, deviceCollection } from "@/lib/collections"
import {
  findStartedRun,
  STARTED_RUN_DEADLINE_MS,
  STARTED_RUN_SKEW_MS,
} from "@/lib/started-run-match"
import { deviceCanResumeRun, deviceRowIsOnline } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { useNow } from "@/hooks/use-now"
import { useOpenSession } from "@/hooks/use-open-session"
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
 * the reason — a conflicted worktree, a failed doctor). */
export function useResumeRun(session: CodingSession): {
  resuming: boolean
  resume: () => Promise<void>
} {
  const [resuming, setResuming] = useState(false)
  const device = useSessionDevice(session)
  const openSession = useOpenSession()
  const [sentAt, setSentAt] = useState<number | null>(null)
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      sentAt !== null ? query.from({ s: codingSessionCollection }) : undefined,
    [sentAt !== null]
  )
  const deadlineRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(
    () => () => {
      if (deadlineRef.current) clearTimeout(deadlineRef.current)
    },
    []
  )
  useEffect(() => {
    if (sentAt === null) return
    const match = findStartedRun(
      (sessionRows ?? []) as CodingSession[],
      { kind: `resumed`, fromId: session.id },
      session.userId,
      sentAt - STARTED_RUN_SKEW_MS
    )
    if (!match) return
    if (deadlineRef.current) clearTimeout(deadlineRef.current)
    setSentAt(null)
    setResuming(false)
    openSession(match)
  }, [sessionRows, sentAt, session.id, session.userId, openSession])

  const resume = async () => {
    if (!session.deviceId) return
    setResuming(true)
    try {
      await trpc.steer.startSession.mutate({
        resumeSessionId: session.id,
        deviceId: session.deviceId,
      })
      setSentAt(Date.now())
      if (deadlineRef.current) clearTimeout(deadlineRef.current)
      deadlineRef.current = setTimeout(() => {
        setSentAt(null)
        setResuming(false)
        toast.error(
          `${device.label ?? `That machine`} never started this run`,
          { description: `Open the Exponential desktop app there to see why.` }
        )
      }, STARTED_RUN_DEADLINE_MS)
    } catch (e) {
      setResuming(false)
      toast.error(e instanceof Error ? e.message : `Could not resume that run`)
    }
  }

  return { resuming, resume }
}
