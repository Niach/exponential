import { useEffect, useRef, useState } from "react"
import { toast } from "sonner"
import { TRPCClientError } from "@trpc/client"
import { trpc } from "@/lib/trpc-client"
import type {
  StartCodingOptions,
  SteerDevice,
} from "@/components/start-coding-dialog"

// Remote "Start on my desktop" (EXP-106): fetches the caller's online desktops
// on mount and delivers a start command through the relay control socket. This
// is the ONE place that knows the batch dispatch shape — a single checked issue
// starts a plain session (`issueId`), 2+ start a BATCH session (`issueIds`).

function trpcErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof TRPCClientError) {
    const message = error.message?.trim()
    if (message && !message.startsWith(`[`) && !message.startsWith(`{`)) {
      return message
    }
  }
  return fallback
}

export interface RemoteCodingStart {
  /** The caller's online desktops; null while the presence lookup is in flight. */
  devices: SteerDevice[] | null
  starting: boolean
  /** Device label a start was just delivered to — cleared after a 30s grace. */
  sentTo: string | null
  /** Resolves on delivery, rejects on failure (toast already shown). */
  start: (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[]
  ) => Promise<void>
}

export function useRemoteCodingStart(): RemoteCodingStart {
  const [devices, setDevices] = useState<SteerDevice[] | null>(null)
  const [starting, setStarting] = useState(false)
  const [sentTo, setSentTo] = useState<string | null>(null)
  const resetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    let active = true
    trpc.steer.myDevices
      .query()
      .then((res) => active && setDevices(res.devices))
      .catch(() => active && setDevices([]))
    return () => {
      active = false
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
    }
  }, [])

  const start = async (
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
      setSentTo(device.deviceLabel)
      // The desktop inserts the coding_sessions row when the launcher spins up,
      // which swaps the caller's start affordance for the live panel via
      // Electric. Re-enable after a grace window in case it never picks up.
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
      resetTimerRef.current = setTimeout(() => setSentTo(null), 30_000)
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

  return { devices, starting, sentTo, start }
}
