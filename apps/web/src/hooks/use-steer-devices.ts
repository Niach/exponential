import { useEffect, useMemo, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import type { Device, User } from "@/db/schema"
import { deviceCollection, userCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { trpc } from "@/lib/trpc-client"
import { composeDeviceList, type SteerDevice } from "@/lib/steer-devices"

// The caller's machines (EXP-403/EXP-432/EXP-481): the synced `devices` shape
// (own rows + teammates' servers shared into the team), online-ness derived
// from `last_seen_at` freshness against a 30s ticking clock, plus the
// instance's `latestVersions` (config, not a shape column) fetched once per
// mount. EXP-1293: this is what is left of `useRemoteStart`; a start is ONE
// `sessions.send` now (`use-launch-composer.ts`), so nothing here watches
// for a run landing.

export interface SteerDevices {
  /** The caller's registered machines (online AND offline); null while the
   *  first sync is in flight. */
  devices: SteerDevice[] | null
  /** Re-fetch `latestVersions` (device rows themselves stream in). */
  refresh: () => void
  /** Informational CLIENT_LATEST_VERSION_* values; null until loaded or
   *  when unset server-side. */
  latestVersions: { desktop: string | null; cli: string | null } | null
}

export function useSteerDevices(
  options: {
    /** Member + relay configured: gates the device list + version fetch. */
    enabled?: boolean
    /** Splits own vs shared rows off the synced devices shape. */
    currentUserId?: string
    /** EXP-432: also list teammates' servers shared with this team. */
    teamId?: string
    /** EXP-1186: list servers shared into ANY of these teams instead (the
     *  phone's Devices tab, cross-team). */
    listTeamIds?: readonly string[]
  } = {}
): SteerDevices {
  const { enabled = true, currentUserId, teamId, listTeamIds } = options
  const listKey = listTeamIds?.join(`,`)
  const [latestVersions, setLatestVersions] = useState<{
    desktop: string | null
    cli: string | null
  } | null>(null)

  const { data: deviceRows } = useLiveQuery(
    (query) => (enabled ? query.from({ d: deviceCollection }) : undefined),
    [enabled]
  )
  const { data: userRows } = useLiveQuery(
    (query) => (enabled ? query.from({ u: userCollection }) : undefined),
    [enabled]
  )
  // 30s tick against the online window (the EXP-153 staleness idiom).
  const now = useNow(30_000)
  const devices = useMemo<SteerDevice[] | null>(() => {
    if (!enabled || !currentUserId || deviceRows === undefined) return null
    const usersById = new Map(
      ((userRows ?? []) as User[]).map((user) => [user.id, user])
    )
    return composeDeviceList(
      deviceRows as Device[],
      usersById,
      now,
      currentUserId,
      listTeamIds && listTeamIds.length > 0 ? listTeamIds : teamId
    )
    // `listKey` stands for `listTeamIds` (a fresh array per render).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, currentUserId, deviceRows, userRows, now, teamId, listKey])

  const [refreshTick, setRefreshTick] = useState(0)
  useEffect(() => {
    if (!enabled) return
    let active = true
    trpc.devices.latestVersions
      .query()
      .then((res) => active && setLatestVersions(res ?? null))
      .catch(() => {})
    return () => {
      active = false
    }
  }, [enabled, refreshTick])

  return {
    devices,
    refresh: () => setRefreshTick((tick) => tick + 1),
    latestVersions,
  }
}
