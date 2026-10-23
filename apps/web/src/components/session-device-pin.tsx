// EXP-1252: the session composer's Device pin — THE launch options line's
// pin (`@exp/ui` DevicePicker in its `inline` variant), placed before Model.
// N-12: inside the composer card, its glyph alone (icon-only trigger).
// A pick sends `sessions.send({ sessionId, deviceId })` with no text: a
// queued row re-targets, a live one ends here and continues on the target
// (NEW-1). Nothing is pending on the row, so there is no "moving" state and
// no cancel (NEW-8). Rules + copy: `lib/session-device-move.ts`.
import { useMemo, useRef } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { DevicePicker, getDeviceIcon, IconTooltip, PickerTrigger } from "@exp/ui"
import type { Device, User } from "@/db/schema"
import { deviceCollection, sessionCollection, userCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import type { SessionRow } from "@/lib/session-tree"
import {
  canSteerSession,
  deviceMoveBlockedReason,
  deviceMovePick,
  deviceMoveVisible,
  NOTE,
  PICKER_TITLE,
  sessionDeviceMoveRows,
} from "@/lib/session-device-move"
import { composeDeviceList } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"

/** How long a move request waits (bounded) for its row to sync. */
const MOVE_SYNC_WAIT_MS = 3_000

export function SessionDevicePin({
  teamId,
  row,
}: {
  teamId: string
  row: Pick<
    SessionRow,
    | `id`
    | `agentId`
    | `deviceId`
    | `shared`
    | `userId`
    | `cli`
    | `status`
  >
}) {
  const { data: auth } = useSession()
  const currentUserId = auth?.user.id
  const now = useNow(30_000)
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const { data: userRows } = useLiveQuery((query) =>
    query.from({ u: userCollection })
  )
  // THE launch composer's machines for this team: own + shared into it.
  const devices = useMemo(() => {
    if (!currentUserId) return []
    const usersById = new Map(
      ((userRows ?? []) as User[]).map((user) => [user.id, user])
    )
    return composeDeviceList(
      (deviceRows ?? []) as Device[],
      usersById,
      now,
      currentUserId,
      teamId
    )
  }, [deviceRows, userRows, now, currentUserId, teamId])

  // One move in flight at a time.
  const sending = useRef(false)

  const rows = useMemo(
    () =>
      sessionDeviceMoveRows(devices, {
        deviceId: row.deviceId ?? null,
        teamId,
        shared: row.shared,
        cli: row.cli,
      }),
    [devices, row.deviceId, teamId, row.shared, row.cli]
  )

  if (!row.deviceId || !deviceMoveVisible(row)) return null

  const blocked = deviceMoveBlockedReason({
    canSteer: canSteerSession(row, currentUserId),
    ended: row.status === `ended`,
  })
  const pick = async (deviceId: string) => {
    if (sending.current) return
    if (deviceMovePick(deviceId, { deviceId: row.deviceId ?? null }) === null) return
    if (rows.find((entry) => entry.id === deviceId)?.reason) return
    sending.current = true
    try {
      const result = await trpc.sessions.send.mutate(
        { teamId, sessionId: row.id, deviceId },
        SKIP_ERROR_TOAST
      )
      const txid = (result as { txid?: number } | undefined)?.txid
      if (txid !== undefined)
        await sessionCollection.utils
          .awaitTxId(txid, MOVE_SYNC_WAIT_MS)
          .catch(() => undefined)
    } catch (error) {
      failedAction(`move`, `the session`, error)
    } finally {
      sending.current = false
    }
  }

  // N-12: the pin is its glyph alone (the name in the picker and the tooltip).
  const current = rows.find((entry) => entry.id === row.deviceId) ?? null
  const picker = (
    <DevicePicker
      triggerVariant="inline"
      mobileTitle={PICKER_TITLE}
      value={row.deviceId}
      devices={rows.map((entry) => ({
        id: entry.id,
        name: entry.name,
        icon: entry.icon,
        kind: entry.kind,
        description: entry.reason ?? undefined,
        disabled: entry.reason !== null,
      }))}
      onChange={(deviceId) => void pick(deviceId)}
      disabled={blocked !== null}
      width="sm"
      trigger={
        <PickerTrigger
          variant="inline"
          iconOnly
          label={PICKER_TITLE}
          value={current?.name ?? row.deviceId}
          icon={getDeviceIcon(current ?? { kind: null })}
          disabled={blocked !== null}
        />
      }
      data-testid="session-composer-device"
    />
  )

  // A refused move keeps its pin and says why, on the pin; an open one
  // says what a move does.
  return (
    <span
      data-testid="session-composer-device-pin"
      title={blocked ? undefined : NOTE}
      className="inline-flex"
    >
      {blocked ? <IconTooltip label={blocked}>{picker}</IconTooltip> : picker}
    </span>
  )
}
