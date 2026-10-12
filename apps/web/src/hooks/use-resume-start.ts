import { useEffect, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { toast, toastActionError } from "@exp/ui"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import type { CodingSession } from "@/db/schema"
import { codingSessionCollection } from "@/lib/collections"
import {
  findStartedRun,
  STARTED_RUN_DEADLINE_MS,
  STARTED_RUN_SKEW_MS,
} from "@/lib/started-run-match"
import { trpc } from "@/lib/trpc-client"
import { useOpenSession } from "@/hooks/use-open-session"
import { newStartId } from "@/lib/start-intent"

// EXP-1259 (start surfaces): the ONE resume send behind the run's Resume
// button (`use-resume-run.ts`) and the account switch
// (`session-account-switch.tsx`, a resume naming an account). A press is a
// new intent and mints a `startId`; every Retry of it (the failure toast's,
// the "never started" toast's) re-sends the SAME id, so the server
// (`forwardStartOnce`) and the device drop a repeat instead of resuming
// twice. The pending state ends at `STARTED_RUN_DEADLINE_MS` (the old-daemon
// fallback: a device below the startId build reports nothing back), but the
// watch keeps following quietly for `starting.watchMs` like the composer's:
// a late row still opens and clears the toast.

const WATCH_MS = FEEDBACK_STATES.starting.watchMs

interface ResumeSend {
  startId: string
  account: string | undefined
  /** `Date.now()` as the FIRST request of this startId went out. */
  requestedAt: number
}

export interface ResumeStart {
  /** The press in flight or waiting for its row (null = idle). */
  pending: { account: string | undefined } | null
  /** A press: a new intent, a new startId. */
  resume: (account?: string) => void
}

export function useResumeStart({
  sessionId,
  userId,
  deviceId,
  deadlineTitle,
  failure,
  canRetry,
  onBeforeSend,
}: {
  sessionId: string
  /** The run's owner: whose resumed row the watch looks for. */
  userId: string
  deviceId: string | null
  /** The "never started" toast's title. */
  deadlineTitle: () => string
  /** The failed send's toast (`Couldn't <verb> <thing>`). */
  failure: { verb: string; thing: string; description?: (error: unknown) => string }
  /** Asked before a Retry re-sends (false = the Retry does nothing). */
  canRetry?: (account: string | undefined) => boolean
  /** Runs right before every request goes out (a press and its Retries). */
  onBeforeSend?: () => void
}): ResumeStart {
  const openSession = useOpenSession()
  const [pending, setPending] = useState<ResumeSend | null>(null)
  // The send the watch follows (outlives `pending` past the deadline).
  const [watch, setWatch] = useState<ResumeSend | null>(null)
  const timersRef = useRef<ReturnType<typeof setTimeout>[]>([])
  const deadlineToastRef = useRef<string | number | null>(null)
  const mountedRef = useRef(true)
  // Read by the press/Retry guards before React re-renders.
  const pendingRef = useRef<ResumeSend | null>(null)
  const setPendingNow = (send: ResumeSend | null) => {
    pendingRef.current = send
    setPending(send)
  }
  const latest = useRef({ deadlineTitle, failure, canRetry, onBeforeSend })
  latest.current = { deadlineTitle, failure, canRetry, onBeforeSend }

  const clearTimers = () => {
    for (const timer of timersRef.current) clearTimeout(timer)
    timersRef.current = []
  }
  const dismissDeadlineToast = () => {
    if (deadlineToastRef.current === null) return
    toast.dismiss(deadlineToastRef.current)
    deadlineToastRef.current = null
  }
  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      clearTimers()
    }
  }, [])

  const watching = watch !== null
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      watching ? query.from({ s: codingSessionCollection }) : undefined,
    [watching]
  )
  useEffect(() => {
    if (!watch) return
    const match = findStartedRun(
      (sessionRows ?? []) as CodingSession[],
      { kind: `resumed`, fromId: sessionId },
      userId,
      watch.requestedAt - STARTED_RUN_SKEW_MS
    )
    if (!match) return
    // Open it exactly once: clearing the watch stops this effect matching.
    clearTimers()
    dismissDeadlineToast()
    setWatch(null)
    setPendingNow(null)
    openSession(match)
  }, [sessionRows, watch, sessionId, userId, openSession])

  /** Sends `send` (a press, or a Retry under the same startId). */
  const deliver = (send: ResumeSend) => {
    if (!deviceId) return
    setPendingNow(send)
    latest.current.onBeforeSend?.()
    trpc.steer.startSession
      .mutate(
        {
          resumeSessionId: sessionId,
          deviceId,
          startId: send.startId,
          ...(send.account !== undefined ? { account: send.account } : {}),
        },
        SKIP_ERROR_TOAST
      )
      .then(() => {
        if (!mountedRef.current) return
        clearTimers()
        dismissDeadlineToast()
        setWatch(send)
        timersRef.current = [
          setTimeout(() => {
            setPendingNow(null)
            deadlineToastRef.current = toastActionError(
              latest.current.deadlineTitle(),
              {
                description: `Open the Exponential desktop app there to see why.`,
                retry: () => retry(send),
              }
            )
          }, STARTED_RUN_DEADLINE_MS),
          setTimeout(() => setWatch(null), WATCH_MS),
        ]
      })
      .catch((error: unknown) => {
        if (!mountedRef.current) return
        setPendingNow(null)
        const { verb, thing, description } = latest.current.failure
        // FEED-79: nothing resumed, so Retry asks the machine again, under
        // the same startId.
        failedAction(verb, thing, error, {
          description: description?.(error),
          retry: () => retry(send),
        })
      })
  }
  const retry = (send: ResumeSend) => {
    if (pendingRef.current) return
    if (latest.current.canRetry && !latest.current.canRetry(send.account)) return
    dismissDeadlineToast()
    deliver(send)
  }

  return {
    pending: pending ? { account: pending.account } : null,
    resume: (account) => {
      if (pendingRef.current || !deviceId) return
      // A new press: a new intent. The previous one's watch (if any) is
      // dropped; its late row only shows in Running.
      clearTimers()
      dismissDeadlineToast()
      setWatch(null)
      deliver({ startId: newStartId(), account, requestedAt: Date.now() })
    },
  }
}
