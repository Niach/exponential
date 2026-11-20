import { useMemo, useSyncExternalStore } from "react"
import { useParams } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { sidebarTree, type SidebarEntry, type SidebarMove } from "@exp/domain-contract/sidebar-folders"
import { sidebarDeviceRows, type SidebarDeviceRow, type SidebarSessionInput } from "@exp/domain-contract/sidebar-tree"
import type { Device, IssueDraft } from "@/db/schema"
import { deviceCollection, issueDraftCollection, pullRequestCollection } from "@/lib/collections"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { resolveDraftEntries } from "@/lib/issue-drafts"
import type { PullRequestRow } from "@/lib/session-tree"
import { useBoardsForTeams } from "@/hooks/use-team-data"
import { useNow } from "@/hooks/use-now"

// #EXP-2027 (fixtures/sidebar-tree.json): the web sidebar tree's data that is
// not one team's: the device-local open flags (Devices, each team), the
// cross-team Drafts count and PRs, the Devices row's children, and the
// registry an Active row's drag drops into a team's list through.

const OPEN_EVENT = `exp:sidebar-tree-open`

function storage(): Storage | null {
  try {
    return typeof window === `undefined` ? null : window.localStorage
  } catch {
    return null
  }
}

function subscribeOpen(callback: () => void) {
  if (typeof window === `undefined`) return () => {}
  window.addEventListener(OPEN_EVENT, callback)
  window.addEventListener(`storage`, callback)
  return () => {
    window.removeEventListener(OPEN_EVENT, callback)
    window.removeEventListener(`storage`, callback)
  }
}

/** Writes a device-local open flag (no stored flag = open). */
export function setSidebarOpenFlag(key: string, open: boolean) {
  const store = storage()
  if (!store) return
  if (open) store.removeItem(key)
  else store.setItem(key, `0`)
  window.dispatchEvent(new Event(OPEN_EVENT))
}

/** A device-local expanded flag (`SIDEBAR_DEVICES_OPEN_KEY`,
 *  `sidebarTeamOpenKey`); open by default. */
export function useSidebarOpenFlag(key: string): [boolean, (open: boolean) => void] {
  const closed = useSyncExternalStore(
    subscribeOpen,
    () => storage()?.getItem(key) === `0`,
    () => false
  )
  return [!closed, (open: boolean) => setSidebarOpenFlag(key, open)]
}

/** A team List's move whose target is the TEAM ROW itself (its id): into
 *  it or after it = the end of the team's list; before it = nothing. Every
 *  other move passes through. `null` with no node to land beside (an empty
 *  team) = append. */
export function teamListMove(
  move: SidebarMove,
  teamId: string,
  items: readonly string[],
  entries: readonly SidebarEntry[]
): { move: SidebarMove } | { append: true } | null {
  const position = move.into !== undefined ? `into` : move.after !== undefined ? `after` : `before`
  const target = move[position]
  if (target !== teamId) return { move }
  if (position === `before`) return null
  const top = sidebarTree(items, entries)
    .map((node) => node.id)
    .filter((id) => id !== move.id)
  const last = top[top.length - 1]
  return last ? { move: { id: move.id, after: last } } : { append: true }
}

/** The caller's parked drafts across `teamIds` (the Drafts row shows while
 *  any): every resolved draft except the one open on the New issue page. */
export function useCrossTeamDraftCount(teamIds: readonly string[]): number {
  const key = teamIds.join(`,`)
  const { draftId } = useParams({ strict: false }) as { draftId?: string }
  const { data } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query.from({ d: issueDraftCollection }).where(({ d }) => inArray(d.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  const { boards } = useBoardsForTeams(teamIds)
  return useMemo(() => {
    const drafts = (data ?? []) as IssueDraft[]
    let count = 0
    for (const teamId of teamIds) {
      count += resolveDraftEntries(drafts, boards ?? [], teamId).filter((entry) => entry.draft.id !== draftId).length
    }
    return count
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `key` stands for teamIds
  }, [data, boards, key, draftId])
}

/** Every member team's PRs grouped by the chat (root) that opened them. */
export function usePullRequestsByRootForTeams(teamIds: readonly string[]): {
  byRoot: ReadonlyMap<string, PullRequestRow[]>
  all: PullRequestRow[]
} {
  const key = teamIds.join(`,`)
  const { data } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query.from({ prs: pullRequestCollection }).where(({ prs }) => inArray(prs.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  return useMemo(() => {
    const all = (data ?? []) as PullRequestRow[]
    const byRoot = new Map<string, PullRequestRow[]>()
    for (const pr of all) {
      if (!pr.rootSessionId) continue
      const list = byRoot.get(pr.rootSessionId) ?? []
      list.push(pr)
      byRoot.set(pr.rootSessionId, list)
    }
    return { byRoot, all }
  }, [data])
}

/** One Devices child: the pure row + what it draws. `id` = the device's
 *  LOGICAL id (`devices.device_id`, what sessions name and the device
 *  pages' `$deviceId` take). */
export interface SidebarDevice extends SidebarDeviceRow {
  name: string
  device: Device
}

/** The Devices row's children (`sidebarDeviceRows`): the viewer's own
 *  devices, then teammates' servers shared with one of `viewerTeamIds`
 *  (the viewer's team order). Online = `last_seen_at` freshness on a 30 s
 *  clock, as the devices list. */
export function useSidebarDevices(me: string | null | undefined, viewerTeamIds: readonly string[]): SidebarDevice[] {
  const { data } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const now = useNow(30_000)
  const key = viewerTeamIds.join(`,`)
  return useMemo(() => {
    if (!me) return []
    const rows = (data ?? []) as Device[]
    const byId = new Map(rows.map((row) => [row.deviceId, row]))
    const devices = rows.map((row) => ({
      id: row.deviceId,
      name: row.label || row.deviceId,
      mine: row.userId === me,
      online: deviceRowIsOnline(row.lastSeenAt, now),
      sharedTeamIds: row.sharedTeamIds ?? [],
    }))
    const names = new Map(devices.map((device) => [device.id, device.name]))
    return sidebarDeviceRows({ devices, viewerTeamIds }).flatMap((row) => {
      const device = byId.get(row.id)
      return device ? [{ ...row, name: names.get(row.id) ?? row.id, device }] : []
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `key` stands for viewerTeamIds
  }, [data, me, now, key])
}

/** An Active row dropped on a team's List (`sidebarDropSession`): each
 *  mounted team list registers its handler here. */
export type SidebarSessionDrop = (session: SidebarSessionInput, move: SidebarMove) => void

const teamDrops = new Map<string, SidebarSessionDrop>()

export function registerSidebarTeamDrop(teamId: string, drop: SidebarSessionDrop): () => void {
  teamDrops.set(teamId, drop)
  return () => {
    if (teamDrops.get(teamId) === drop) teamDrops.delete(teamId)
  }
}

export function sidebarTeamDrop(teamId: string): SidebarSessionDrop | null {
  return teamDrops.get(teamId) ?? null
}

/** The DOM hook of a team's List: its rows are what an Active row's drag
 *  drops between. */
export const SIDEBAR_TEAM_LIST_ATTR = `data-sidebar-team-list`
