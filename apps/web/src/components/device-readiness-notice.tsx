// EXP-1196/1218: the app wiring around `@exp/ui` `DeviceReadiness` — every
// web surface shows ANOTHER device (web has no local machine), so actions are
// the remote ones: `update` queues `agent_update`, `sign_in` opens the shared
// remote login dialog (`requestAgentLogin`, EXP-862).
import { useCallback } from "react"
import { DeviceReadiness, deviceReadinessBlocker, toast } from "@exp/ui"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import type { SteerDevice } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"

/** The remote action handler for `device`'s readiness rows. */
export function useDeviceReadinessAction(
  device: SteerDevice | null | undefined
): (itemKey: string, action: string) => void {
  return useCallback(
    (itemKey: string, action: string) => {
      if (!device) return
      if (action === `sign_in`) {
        requestAgentLogin({ device, agent: itemKey })
        return
      }
      if (action === `update`) {
        void trpc.devices.createCommand
          .mutate(
            { deviceId: device.deviceId, kind: `agent_update`, agent: itemKey },
            { context: { skipErrorToast: true } }
          )
          .then(() => toast.success(`Update queued on ${device.deviceLabel || device.deviceId}`))
          .catch((error: unknown) =>
            toast.error(trpcErrorMessage(error, `Couldn't queue that on the device.`))
          )
      }
    },
    [device]
  )
}

/** The device's full block, or (`problemsOnly`) just the rows that need
 *  something. Renders nothing for an older build (no doctor). */
export function DeviceReadinessBlock({
  device,
  problemsOnly,
  className,
}: {
  device: SteerDevice
  problemsOnly?: boolean
  className?: string
}) {
  const onAction = useDeviceReadinessAction(device)
  return (
    <DeviceReadiness
      doctor={device.doctor}
      remote
      problemsOnly={problemsOnly}
      onAction={onAction}
      className={className}
    />
  )
}

/** The composer's notice for a picked device + agent that cannot start: the
 *  failing doctor ROW (Git, else the agent's own) with its action; the old
 *  sentence when the device reports no doctor; nothing when ready. */
export function DeviceReadinessNotice({
  device,
  agent,
  notReady,
  className,
}: {
  device: SteerDevice | null | undefined
  agent: string
  /** The ACP predicate (`deviceAgentNotReady`) — the fallback's trigger. */
  notReady: boolean
  className?: string
}) {
  const onAction = useDeviceReadinessAction(device)
  if (!device) return null
  const blocker = deviceReadinessBlocker(device.doctor, agent)
  if (blocker) {
    return (
      <DeviceReadiness
        doctor={device.doctor}
        remote
        only={[blocker]}
        onAction={onAction}
        className={className}
      />
    )
  }
  if (!notReady) return null
  return (
    <span className={className}>
      {`Not ready on ${device.deviceLabel || device.deviceId}. Run the doctor there.`}
    </span>
  )
}
