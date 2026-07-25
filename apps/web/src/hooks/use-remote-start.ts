import { useEffect, useRef, useState } from "react"
import { toast } from "sonner"
import { TRPCClientError } from "@trpc/client"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { CodingSession } from "@/db/schema"
import { codingSessionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { isBuiltinActionId } from "@/lib/builtin-actions"
import type { CodingLaunchPrefs } from "@/lib/coding-launch-prefs"
import type { SteerDevice } from "@/lib/steer-devices"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"

// Remote "Start on my desktop" (EXP-106/EXP-253, merged in EXP-257): fetch
// the caller's online desktops, then deliver a start command through the
// relay control socket — a single-issue session (`issueId`), a BATCH session
// (`issueIds`, 2+), or an action run (`actionId`). After an action send the
// hook watches the synced coding_sessions rows for the desktop's run and
// focuses the dock on it once. The watch matches on `actionName` (+ own
// userId + startedAt window), NEVER actionId — the builtin "Create action"
// run is inserted with actionId NULL.

/** The resolved launch-dialog choices sent with `steer.startSession` — the
 * same shape the prefs module persists. */
export type StartCodingOptions = CodingLaunchPrefs

/** The minimal action identity a run needs: `teamId` rides the mutation only
 * for the builtin (there is no DB row to derive the team from), and `name`
 * keys the post-send dock watch. */
export interface RemoteStartAction {
  id: string
  name: string
  teamId: string
}

function trpcErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof TRPCClientError) {
    const message = error.message?.trim()
    if (message && !message.startsWith(`[`) && !message.startsWith(`{`)) {
      return message
    }
  }
  return fallback
}

export interface RemoteStart {
  /** The caller's online desktops; null while the presence lookup is in flight. */
  devices: SteerDevice[] | null
  starting: boolean
  /** Device label a start was just delivered to — cleared once an action
   * run's synced row appears (dock auto-focused) or after a 30s grace. */
  sentTo: string | null
  /** Resolves on delivery, rejects on failure (toast already shown). */
  startIssues: (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[]
  ) => Promise<void>
  /** Resolves on delivery, rejects on failure (toast already shown). */
  runAction: (
    device: SteerDevice,
    action: RemoteStartAction,
    options: StartCodingOptions,
    inputs?: Record<string, string>
  ) => Promise<void>
}

export function useRemoteStart({
  enabled = true,
  currentUserId,
}: {
  /** Member + relay configured — gates the myDevices presence fetch. */
  enabled?: boolean
  /** Keys the action dock watch to the caller's own runs. */
  currentUserId?: string
} = {}): RemoteStart {
  const [devices, setDevices] = useState<SteerDevice[] | null>(null)
  const [starting, setStarting] = useState(false)
  const [pending, setPending] = useState<{
    deviceLabel: string
    sentAt: number
    /** Set for action sends only — arms the dock watch below. */
    actionName?: string
  } | null>(null)
  const resetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const dock = useAgentDock()

  useEffect(() => {
    if (!enabled) return
    let active = true
    trpc.steer.myDevices
      .query()
      .then((res) => active && setDevices(res.devices))
      .catch(() => active && setDevices([]))
    return () => {
      active = false
    }
  }, [enabled])

  useEffect(
    () => () => {
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
    },
    []
  )

  // Watch the synced coding_sessions rows for the desktop picking an action
  // start up: a fresh row by ME with the started action's name snapshot. The
  // 60s skew allowance absorbs client/server clock drift on `startedAt`.
  const pendingActionName = pending?.actionName
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      pendingActionName
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.actionName, pendingActionName))
        : undefined,
    [pendingActionName]
  )

  useEffect(() => {
    if (!pending?.actionName || !currentUserId) return
    const match = ((sessionRows ?? []) as CodingSession[]).find(
      (s) =>
        s.userId === currentUserId &&
        new Date(s.startedAt).getTime() >= pending.sentAt - 60_000
    )
    if (!match) return
    // Focus the dock exactly once — clearing `pending` stops this effect from
    // ever matching again for this send.
    dock?.openDock(match.id)
    if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
    setPending(null)
  }, [sessionRows, pending, currentUserId, dock])

  // The desktop inserts the coding_sessions row when the launcher spins up;
  // re-enable the affordances after a grace window in case it never picks up.
  const markSent = (deviceLabel: string, actionName?: string) => {
    if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
    setPending({ deviceLabel, sentAt: Date.now(), actionName })
    resetTimerRef.current = setTimeout(() => setPending(null), 30_000)
  }

  const startIssues = async (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[]
  ) => {
    if (issueIds.length === 0) return
    setStarting(true)
    try {
      const base = { deviceId: device.deviceId, ...options }
      await trpc.steer.startSession.mutate(
        // 1 issue → plain single-issue session; 2+ → one batch session on a
        // single pushed branch (the server contract owns the fan-out).
        issueIds.length === 1
          ? { issueId: issueIds[0], ...base }
          : { issueIds, ...base },
        { context: { skipErrorToast: true } }
      )
      markSent(device.deviceLabel)
    } catch (error) {
      toast.error(`Couldn't start on your desktop`, {
        description: trpcErrorMessage(
          error,
          `The start command could not be delivered`
        ),
      })
      throw error
    } finally {
      setStarting(false)
    }
  }

  const runAction = async (
    device: SteerDevice,
    action: RemoteStartAction,
    options: StartCodingOptions,
    inputs?: Record<string, string>
  ) => {
    setStarting(true)
    try {
      await trpc.steer.startSession.mutate(
        {
          actionId: action.id,
          deviceId: device.deviceId,
          // teamId is required iff the action is the virtual builtin (no DB
          // row to derive the team from) and forbidden otherwise.
          ...(isBuiltinActionId(action.id) ? { teamId: action.teamId } : {}),
          ...(inputs ? { inputs } : {}),
          ...options,
        },
        { context: { skipErrorToast: true } }
      )
      markSent(device.deviceLabel, action.name)
    } catch (error) {
      toast.error(`Couldn't run the action on your desktop`, {
        description: trpcErrorMessage(
          error,
          `The start command could not be delivered`
        ),
      })
      throw error
    } finally {
      setStarting(false)
    }
  }

  return {
    devices,
    starting,
    sentTo: pending?.deviceLabel ?? null,
    startIssues,
    runAction,
  }
}
