import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { DevicePicker, Prompt, type DevicePickerDevice } from "@exp/ui"
import { runsOnRows } from "@exp/domain-contract/agent-settings"
import { DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"
import { MEMBER_SPACE_COPY, spaceSections } from "@exp/domain-contract/member-space"
import type { Device, User } from "@/db/schema"
import { deviceCollection, userCollection } from "@/lib/collections"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { memberSpacePickPrompt, promptActions } from "@/lib/prompts"
import { deviceIsOnline, deviceRowIsOnline, steerDeviceFromRow, type SteerDevice } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"

// #EXP-2161 (member-space.json, UI ×4): the Assistant page's Runs on device
// row: the member's own machines (`runsOnRows`), then one section per team
// with opted-in hosts of others (`spaceSections`), headed by the team's name.
// Picking one of those confirms (`copy.pick*`) and is
// `memberSpaces.request`; a bound space shows as its host's row. An Assistant
// without a device (its space ended) reads `copy.noDevice`.

/** The synced columns the sections read. */
export type SpaceRow = Pick<Device, `id` | `deviceId` | `userId` | `label` | `lastSeenAt` | `icon` | `kind`> & {
  assistantTeamIds?: string[] | null
  spaceHostId?: string | null
  spaceTeamId?: string | null
}

/** A team device's picker row; `value` names the team too (one host may be
 *  listed under several). */
export interface SpacePick extends DevicePickerDevice {
  teamId: string
  hostDeviceId: string
  ownerId: string
  selected: boolean
}

/** The picker rows of `spaceSections`, in order, and the picked one's value. */
export function spacePicks(
  rows: readonly SpaceRow[],
  teams: readonly { id: string; name: string }[],
  boundId: string | null,
  me: string,
  now: Date
): { picks: SpacePick[]; value: string | null } {
  const sections = spaceSections(
    rows.map((row) => ({
      rowId: row.id,
      deviceId: row.deviceId,
      userId: row.userId,
      label: row.label,
      online: deviceRowIsOnline(row.lastSeenAt, now),
      assistantTeamIds: row.assistantTeamIds ?? [],
      spaceHostId: row.spaceHostId ?? null,
    })),
    teams,
    boundId,
    me
  )
  const picks = sections.flatMap((section) =>
    section.rows.map((row): SpacePick => {
      const host = rows.find(
        (device) => device.deviceId === row.id && device.userId !== me && (device.assistantTeamIds ?? []).includes(section.teamId)
      )
      const online = host ? deviceRowIsOnline(host.lastSeenAt, now) : false
      return {
        id: `${section.teamId}:${row.id}`,
        name: row.label || row.id,
        icon: host?.icon,
        kind: host?.kind ?? `server`,
        section: section.title,
        teamId: section.teamId,
        hostDeviceId: row.id,
        ownerId: host?.userId ?? ``,
        selected: row.selected,
        checked: row.selected,
        disabled: row.disabled,
        ...(online ? {} : { description: DEVICES_LIST_COPY.offline }),
      }
    })
  )
  const space = rows.find((row) => row.deviceId === boundId && row.spaceHostId)
  const picked = picks.filter((pick) => pick.selected)
  const value = (picked.find((pick) => pick.teamId === space?.spaceTeamId) ?? picked[0])?.id ?? null
  return { picks, value }
}

/** The raw rows (a space resolves here, never in a list) and the team rows. */
export function useSpaceRows(boundId: string | null, enabled = true) {
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? ``
  const { teams } = useMemberTeams(enabled && me ? me : null)
  const { data } = useLiveQuery((query) => (enabled ? query.from({ d: deviceCollection }) : undefined), [enabled])
  const { data: userRows } = useLiveQuery((query) => (enabled ? query.from({ u: userCollection }) : undefined), [enabled])
  const now = useNow(30_000)
  return useMemo(() => {
    const rows = (data ?? []) as Device[]
    const users = new Map(((userRows ?? []) as User[]).map((user) => [user.id, user.name || user.email]))
    const space = rows.find((row) => row.deviceId === boundId && row.spaceHostId) ?? null
    const host = space ? (rows.find((row) => row.id === space.spaceHostId) ?? null) : null
    return { me, space, host, users, ...spacePicks(rows, teams, boundId, me, now) }
  }, [data, userRows, teams, boundId, me, now])
}

/** The device the Account and Model rows read: the bound space's own row
 *  (its logins are the host's), the host's while it reports none yet. */
export function spaceDevice(space: Device, host: Device | null, me: string, now: Date): SteerDevice {
  const own = steerDeviceFromRow(space, { now, currentUserId: me })
  if (!host || (own.agentAccounts && Object.keys(own.agentAccounts).length > 0)) return own
  const { agents, unauthedAgents, acpAgents, agentAccounts, agentUsage, agentUsageAt } = steerDeviceFromRow(host, { now, currentUserId: me })
  return { ...own, agents, unauthedAgents, acpAgents, agentAccounts, agentUsage, agentUsageAt }
}

/** How often a space that is not let in yet is read again. */
const WAITING_INTERVAL_MS = 15_000

/** Whether the bound space's install waits for this member's approval. */
export function useSpaceWaiting(spaceId: string | null): boolean {
  const [waiting, setWaiting] = useState<string | null>(null)
  useEffect(() => {
    if (!spaceId) return
    let live = true
    let timer: ReturnType<typeof setTimeout> | undefined
    const read = () =>
      void trpc.memberSpaces.mine
        .query(undefined, SKIP_ERROR_TOAST)
        .then(({ space }) => {
          if (!live) return
          const status = space?.spaceId === spaceId ? space.status : null
          setWaiting(status === `waiting` ? spaceId : null)
          if (status !== `ready`) timer = setTimeout(read, WAITING_INTERVAL_MS)
        })
        .catch(() => {
          if (live) timer = setTimeout(read, WAITING_INTERVAL_MS)
        })
    read()
    return () => {
      live = false
      clearTimeout(timer)
    }
  }, [spaceId])
  return spaceId !== null && waiting === spaceId
}

export function AssistantDeviceRow({
  deviceId,
  devices,
  onPickOwn,
  onPickedSpace,
}: {
  /** The Assistant's device (null = none: it ended, or none exists yet). */
  deviceId: string | null
  /** The member's own machines. */
  devices: readonly SteerDevice[]
  onPickOwn: (deviceId: string) => void
  /** `memberSpaces.request` went through: the space's device id. */
  onPickedSpace: (deviceId: string) => void
}): ReactNode {
  const { space, users, picks, value } = useSpaceRows(deviceId)
  const waiting = useSpaceWaiting(space?.id ?? null)
  const [confirm, setConfirm] = useState<SpacePick | null>(null)
  const [busy, setBusy] = useState(false)
  // Kept through the close animation.
  const [shown, setShown] = useState<SpacePick | null>(null)
  const own = useMemo(() => runsOnPickerRows(devices, space ? null : deviceId, true), [devices, space, deviceId])
  // A space whose host is gone from the list names itself.
  const orphan: DevicePickerDevice[] =
    space && value === null ? [{ id: space.deviceId, name: space.label || space.deviceId, icon: space.icon, kind: space.kind }] : []

  const request = useCallback(async () => {
    if (!confirm) return
    setBusy(true)
    try {
      const result = await trpc.memberSpaces.request.mutate({ deviceId: confirm.hostDeviceId, teamId: confirm.teamId }, SKIP_ERROR_TOAST)
      await deviceCollection.utils.awaitTxId(result.txid).catch(() => {})
      onPickedSpace(result.deviceId)
      setConfirm(null)
    } catch (error) {
      failedAction(`save`, `the device`, error)
    } finally {
      setBusy(false)
    }
  }, [confirm, onPickedSpace])

  const pick = (next: string) => {
    const target = picks.find((row) => row.id === next)
    if (!target) return onPickOwn(next)
    if (target.selected) return
    setShown(target)
    setConfirm(target)
  }

  const copy = memberSpacePickPrompt(shown?.name ?? ``, users.get(shown?.ownerId ?? ``) ?? ``)
  return (
    <>
      <DevicePicker
        triggerVariant="row"
        mobileTitle="Device"
        value={space ? (value ?? space.deviceId) : deviceId}
        onChange={pick}
        devices={[...orphan, ...own, ...picks]}
        triggerLabel={MEMBER_SPACE_COPY.noDevice}
        emptyText="No devices online"
        data-testid="agent-device"
      />
      {waiting && (
        <div className="px-4 py-3 text-sm text-foreground/50" data-testid="assistant-space-waiting">
          {MEMBER_SPACE_COPY.waiting}
        </div>
      )}
      <Prompt
        open={confirm !== null}
        onOpenChange={(open) => {
          if (!open) setConfirm(null)
        }}
        busy={busy}
        title={copy.title}
        body={copy.body}
        actions={promptActions(copy, { pick: { busy, onSelect: request, testId: `assistant-space-confirm` } })}
      />
    </>
  )
}

/** The Device row's entries (`runsOnRows`, `runsOnRowCases`): online
 *  machines are pickable, an offline one stays listed (disabled unless bound)
 *  so a bound agent still names it, and a binding the viewer cannot see (a
 *  teammate's private machine) renders by its raw id.
 *  NEW-99: an Assistant's lists desktops only (`runsOnDeviceIds`). */
export function runsOnPickerRows(all: readonly SteerDevice[], boundId: string | null, assistant: boolean): DevicePickerDevice[] {
  const byId = new Map(all.map((device) => [device.deviceId, device]))
  return runsOnRows(
    all.map((device) => ({ deviceId: device.deviceId, kind: device.kind, online: deviceIsOnline(device) })),
    boundId,
    assistant
  ).map((row) => {
    const device = byId.get(row.id)
    if (!device) return { id: row.id, name: row.id }
    return {
      id: row.id,
      name: device.deviceLabel || device.deviceId,
      icon: device.icon,
      kind: device.kind,
      ...(row.description ? { description: row.description, disabled: row.disabled } : {}),
    }
  })
}
