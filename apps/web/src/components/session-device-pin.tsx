// EXP-1252: the session composer's Device pin — THE launch options line's
// pin (`@exp/ui` DevicePicker in its `inline` variant), placed before Model.
// N-12: inside the composer card, its glyph alone (icon-only trigger).
// A pick sends `sessions.send({ sessionId, deviceId })` with no text: the
// source device hands the SAME session over after its current turn. While
// `moveToDeviceId` is set the pin reads "Moving to <device>…"; picking the
// current device then cancels. Rules + copy: `lib/session-device-move.ts`.
import { useMemo, useState } from "react"
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
  movingLabel,
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
    | `kind`
    | `aside`
    | `deviceId`
    | `moveToDeviceId`
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

  // A pick reads at once until the row confirms it (or the call fails).
  const [pending, setPending] = useState<{ target: string | null } | null>(null)
  const moveToDeviceId = pending ? pending.target : (row.moveToDeviceId ?? null)

  const rows = useMemo(
    () =>
      sessionDeviceMoveRows(devices, {
        deviceId: row.deviceId ?? null,
        moveToDeviceId,
        teamId,
        shared: row.shared,
        cli: row.cli,
      }),
    [devices, row.deviceId, moveToDeviceId, teamId, row.shared, row.cli]
  )

  if (!row.deviceId || !deviceMoveVisible(row)) return null

  const blocked = deviceMoveBlockedReason({
    canSteer: canSteerSession(row, currentUserId),
    ended: row.status === `ended`,
  })
  // A target outside the list (not shared with the viewer) reads as its id.
  const target = moveToDeviceId
    ? (rows.find((entry) => entry.id === moveToDeviceId) ?? {
        id: moveToDeviceId,
        name: moveToDeviceId,
        icon: null,
        kind: null,
      })
    : null

  const pick = async (deviceId: string) => {
    const action = deviceMovePick(deviceId, {
      deviceId: row.deviceId ?? null,
      moveToDeviceId,
    })
    if (action === null) return
    if (action === `move` && rows.find((entry) => entry.id === deviceId)?.reason) return
    setPending({ target: action === `move` ? deviceId : null })
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
      setPending(null)
    }
  }

  // N-12: the pin is its glyph alone (the name in the picker and the
  // tooltip); a pending move names the target there instead.
  const current = rows.find((entry) => entry.id === row.deviceId) ?? null
  const shown = target ?? current
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
          value={target ? movingLabel(target.name) : (current?.name ?? row.deviceId)}
          icon={getDeviceIcon(shown ?? { kind: null })}
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
      data-moving={target ? `true` : undefined}
      title={blocked ? undefined : NOTE}
      className="inline-flex"
    >
      {blocked ? <IconTooltip label={blocked}>{picker}</IconTooltip> : picker}
    </span>
  )
}
