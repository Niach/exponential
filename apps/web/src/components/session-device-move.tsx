// #EXP-1703: the offline banner's "Move" (session-card.json `bannerActions`):
// THE device picker (`@exp/ui` DevicePicker) behind a button, the rows and
// refusals of `lib/session-device-move.ts` (session-device-move.json ×4), a
// pick = `sessions.send({ sessionId, deviceId })` (the server's move).
import { useMemo, useState } from "react"
import { Button, conceptIcon, DevicePicker, type DevicePickerDevice } from "@exp/ui"
import { useSession } from "@/hooks/use-session"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { canSteerSession } from "@/lib/session-account-switch"
import {
  deviceMoveBlockedReason,
  deviceMovePick,
  deviceMoveVisible,
  PICKER_TITLE,
  sessionDeviceMoveRows,
} from "@/lib/session-device-move"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

const MoveIcon = conceptIcon(`ui-swap`)

type MoveSession = Pick<
  SessionRow,
  `id` | `teamId` | `cli` | `deviceId` | `userId` | `shared` | `status` | `agentId`
>

export function SessionDeviceMoveButton({ session, label }: { session: MoveSession; label: string }) {
  const { data: auth } = useSession()
  const viewerId = auth?.user.id ?? null
  const { devices } = useSteerDevices({ currentUserId: viewerId ?? undefined, teamId: session.teamId })
  const [moving, setMoving] = useState(false)
  const rows = useMemo<DevicePickerDevice[]>(
    () =>
      sessionDeviceMoveRows(devices ?? [], session, viewerId).map((row) => ({
        id: row.id,
        name: row.name,
        icon: row.icon,
        kind: row.kind,
        ...(row.reason ? { description: row.reason, disabled: true } : {}),
      })),
    [devices, session, viewerId]
  )
  const blocked = deviceMoveBlockedReason({
    canSteer: canSteerSession(session, viewerId ?? undefined),
    ended: session.status === `ended`,
  })
  if (blocked || !deviceMoveVisible(session)) return null

  const moveTo = async (deviceId: string) => {
    if (deviceMovePick(deviceId, session) !== `move`) return
    setMoving(true)
    try {
      await trpc.sessions.send.mutate({ sessionId: session.id, deviceId }, SKIP_ERROR_TOAST)
    } catch (error) {
      failedAction(`move`, `the session`, error)
    } finally {
      setMoving(false)
    }
  }

  return (
    <DevicePicker
      devices={rows}
      value={session.deviceId}
      onChange={(deviceId) => void moveTo(deviceId)}
      mobileTitle={PICKER_TITLE}
      align="end"
      trigger={
        <Button
          variant="outline"
          size="xs"
          className="shrink-0"
          disabled={moving}
          pending={moving}
          data-testid="session-device-move"
        >
          <MoveIcon />
          {label}
        </Button>
      }
    />
  )
}
