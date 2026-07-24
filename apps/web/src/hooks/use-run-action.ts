import { useEffect, useRef, useState } from "react"
import { toast } from "sonner"
import { TRPCClientError } from "@trpc/client"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { CodingSession } from "@/db/schema"
import { codingSessionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import type { SteerDevice } from "@/components/start-coding-dialog"

// Remote "Run action on my desktop" (EXP-253) — the action twin of
// use-remote-coding-start: fetch the caller's online desktops, deliver a
// start command through the relay, then watch the synced coding_sessions
// rows for the desktop's action run and focus the dock on it once. Action
// runs are Claude-only v1, so the launch options are model/effort alone.

/** Only desktops advertising this capability have an action launch path —
 * older builds can run claude but not actions (`steer.startSession` enforces
 * the same server-side). */
export function deviceCanRunActions(device: SteerDevice): boolean {
  return (device.caps ?? []).includes(`actions`)
}

export interface RunActionOptions {
  /** Claude model; absent = the desktop's settings default. */
  model?: string
  /** Claude effort; absent = the desktop's settings default. */
  effort?: string
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

export interface RunActionState {
  /** The caller's online desktops (unfiltered — callers gate on
   * deviceCanRunActions); null while the presence lookup is in flight. */
  devices: SteerDevice[] | null
  starting: boolean
  /** Device label a start was just delivered to — cleared once the run's
   * synced row appears (dock auto-focused) or after a 30s grace. */
  sentTo: string | null
  /** Resolves on delivery, rejects on failure (toast already shown). */
  start: (
    device: SteerDevice,
    actionId: string,
    options: RunActionOptions
  ) => Promise<void>
}

export function useRunAction({
  enabled,
  currentUserId,
}: {
  /** Member + relay configured — gates the myDevices presence fetch. */
  enabled: boolean
  currentUserId: string | undefined
}): RunActionState {
  const [devices, setDevices] = useState<SteerDevice[] | null>(null)
  const [starting, setStarting] = useState(false)
  const [pending, setPending] = useState<{
    actionId: string
    sentAt: number
    deviceLabel: string
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

  // Watch the synced coding_sessions rows for the desktop picking the start
  // up: a fresh row by ME for the started action. The 60s skew allowance
  // absorbs client/server clock drift on `startedAt`.
  const pendingActionId = pending?.actionId
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      pendingActionId
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.actionId, pendingActionId))
        : undefined,
    [pendingActionId]
  )

  useEffect(() => {
    if (!pending || !currentUserId) return
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

  const start = async (
    device: SteerDevice,
    actionId: string,
    options: RunActionOptions
  ) => {
    setStarting(true)
    try {
      await trpc.steer.startSession.mutate(
        {
          actionId,
          deviceId: device.deviceId,
          ...(options.model ? { model: options.model } : {}),
          ...(options.effort ? { effort: options.effort } : {}),
        },
        { context: { skipErrorToast: true } }
      )
      // The desktop inserts the coding_sessions row when the launcher spins
      // up — the watcher above focuses the dock on it. Re-enable after a
      // grace window in case it never picks up.
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
      setPending({
        actionId,
        sentAt: Date.now(),
        deviceLabel: device.deviceLabel,
      })
      resetTimerRef.current = setTimeout(() => setPending(null), 30_000)
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

  return { devices, starting, sentTo: pending?.deviceLabel ?? null, start }
}
