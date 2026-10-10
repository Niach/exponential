// EXP-1196/1218: the app wiring around `@exp/ui` `DeviceReadiness` — every
// web surface shows ANOTHER device (web has no local machine), so actions are
// the remote ones: `update` queues `agent_update`, `sign_in` opens the shared
// remote login dialog (`requestAgentLogin`, EXP-862), and a confirmed `import`
// queues `agent_login {import: true}` (cap `agent-import`), which MOVES the
// agent CLI's ambient login into one of the machine's profiles.
import { useCallback } from "react"
import { DeviceReadiness, deviceReadinessBlocker, toast } from "@exp/ui"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import { alreadyAddedCopy } from "@/components/device-agent-account"
import {
  deviceCanImportAgentLogin,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"

/** How long an import waits for the machine to finish it before going quiet
 * (the row moves on the next heartbeat either way). */
const IMPORT_WAIT_MS = 120_000

/** Queue the import of `agent`'s ambient login (`email`, the doctor item's
 * `import`) on `device` and report how it ended. An import has no intended
 * profile, so an email the machine already holds is refreshed: the
 * `alreadyAdded` warning instead of the device's own `Imported {email}.` */
export async function queueAgentImport(
  device: SteerDevice,
  agent: string,
  email: string
): Promise<void> {
  const known = (device.agentAccounts?.[agent]?.profiles ?? []).some(
    (profile) =>
      profile?.email?.trim().toLowerCase() === email.trim().toLowerCase()
  )
  let commandId: string
  try {
    const queued = await trpc.devices.createCommand.mutate(
      {
        deviceId: device.deviceId,
        kind: `agent_login`,
        agent: agent as never,
        import: true,
      },
      { context: { skipErrorToast: true } }
    )
    commandId = queued.id
  } catch (error) {
    toast.error(trpcErrorMessage(error, `Couldn't queue that on the device.`))
    return
  }
  const deadline = Date.now() + IMPORT_WAIT_MS
  const interval = deviceIsOnline(device) ? 2_000 : 8_000
  while (Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, interval))
    let command
    try {
      command = await trpc.devices.getCommand.query({ commandId })
    } catch {
      continue // Transient — keep polling.
    }
    if (command.status === `pending`) continue
    if (command.status === `failed`) {
      toast.error(command.result ?? `The device reported a failure.`)
    } else if (known) {
      toast.warning(alreadyAddedCopy(email))
    } else {
      toast.success(command.result ?? `Imported ${email}.`)
    }
    return
  }
}

/** The remote action handler for `device`'s readiness rows. */
export function useDeviceReadinessAction(
  device: SteerDevice | null | undefined
): (itemKey: string, action: string) => void {
  return useCallback(
    (itemKey: string, action: string) => {
      if (!device) return
      if (action === `import`) {
        const email = device.doctor?.items.find((item) => item.key === itemKey)
          ?.import
        if (email) void queueAgentImport(device, itemKey, email)
        return
      }
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
      canImport={deviceCanImportAgentLogin(device)}
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
        canImport={deviceCanImportAgentLogin(device)}
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
