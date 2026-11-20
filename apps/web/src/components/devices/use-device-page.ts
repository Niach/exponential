import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import type { Device, Team, User } from "@/db/schema"
import { deviceCollection, userCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { steerDeviceFromRow, type SteerDevice } from "@/lib/steer-devices"

// #EXP-2027 (sidebar-tree.json DEVICES): the device a device page shows. The
// param is the sidebar row's id: the devices row id, or (an old link, an
// inbox row) the machine's logical `device_id`. The viewer sees it when it is
// their own machine or a teammate's SERVER shared with one of their teams
// (`sidebarDeviceRows`); anything else is not found.

/** The visible device row for `param`, else null (pure). */
export function findPageDevice(
  rows: readonly Device[],
  param: string,
  viewer: { userId: string; teamIds: readonly string[] }
): Device | null {
  const row = rows.find((device) => device.id === param) ?? rows.find((device) => device.deviceId === param)
  if (!row) return null
  if (row.userId === viewer.userId) return row
  const shared = row.kind === `server` && (row.sharedTeamIds ?? []).some((id) => viewer.teamIds.includes(id))
  return shared ? row : null
}

export interface DevicePageData {
  /** The device, live; null = not (yet) found. */
  device: SteerDevice | null
  /** The devices shape has answered: a null `device` is final. */
  ready: boolean
  /** The viewer owns it (accounts, settings). */
  mine: boolean
  /** The viewer's member teams (the running rows' team marks). */
  teams: Team[]
  latestVersions: { desktop: string | null; cli: string | null } | null
  refresh: () => void
}

export function useDevicePage(param: string, userId: string): DevicePageData {
  const { myTeams } = useTeamMemberships(userId)
  const teamIds = useMemo(() => myTeams.map((team) => team.id), [myTeams])
  const { data: deviceRows, isReady } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const { data: userRows } = useLiveQuery((query) => query.from({ u: userCollection }), [])
  // The versions the Update line compares against (and the refresh beat).
  const steer = useSteerDevices({ enabled: true, currentUserId: userId })
  const now = useNow(30_000)
  const row = useMemo(
    () => findPageDevice((deviceRows ?? []) as Device[], param, { userId, teamIds }),
    [deviceRows, param, userId, teamIds]
  )
  const device = useMemo(() => {
    if (!row) return null
    const owner = ((userRows ?? []) as User[]).find((user) => user.id === row.userId)
    return steerDeviceFromRow(row, { now, currentUserId: userId, ownerName: owner?.name })
  }, [row, userRows, now, userId])
  return {
    device,
    ready: isReady,
    mine: row?.userId === userId,
    teams: myTeams as Team[],
    latestVersions: steer.latestVersions,
    refresh: steer.refresh,
  }
}
