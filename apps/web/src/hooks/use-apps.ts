import { useMemo } from "react"
import { and, eq, isNull, useLiveQuery } from "@tanstack/react-db"
import { sortApps } from "@exp/domain-contract/apps"
import type { App, SyncedAppVersion } from "@/db/schema"
import { appCollection, appVersionCollection } from "@/lib/collections"
import { useTeamSessions } from "@/hooks/use-session-tree"
import type { SessionRow } from "@/lib/session-tree"

// VAPP-110: a team's APPS off the synced `apps` shape (trashed rows hidden),
// their versions off `app_versions`, and a member's EDITOR SESSION for one
// (a root with `appId` + `userId` = me) off the sessions shape.

/** A list entry: one synced app (Exponential ships no built-in apps). */
export interface AppListEntry {
  id: string
  slug: string
  name: string
  icon: string | null
  color: string | null
  description: string
  headVersion: number
  createdBy: string | null
  row: App | null
}

export function toListEntry(row: App): AppListEntry {
  return {
    id: row.id,
    slug: row.slug,
    name: row.name,
    icon: row.icon,
    color: row.color,
    description: row.description,
    headVersion: row.headVersion,
    createdBy: row.createdBy,
    row,
  }
}

export function useApps(teamId: string | null | undefined): { apps: AppListEntry[]; isReady: boolean } {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ a: appCollection })
            .where(({ a }) => and(eq(a.teamId, teamId), isNull(a.deletedAt)))
        : undefined,
    [teamId]
  )
  const apps = useMemo(
    () => sortApps(((data ?? []) as App[]).map(toListEntry)),
    [data]
  )
  return { apps, isReady: Boolean(teamId) && isReady }
}

/** One app by slug. `null` while the shape has
 *  no such row (it may still be syncing: the caller waits a grace). */
export function useApp(teamId: string | null | undefined, slug: string): { app: AppListEntry | null; isReady: boolean } {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ a: appCollection })
            .where(({ a }) => and(eq(a.teamId, teamId), eq(a.slug, slug), isNull(a.deletedAt)))
        : undefined,
    [teamId, slug]
  )
  const app = useMemo(() => {
    const row = ((data ?? []) as App[])[0]
    return row ? toListEntry(row) : null
  }, [data, slug])
  return { app, isReady: Boolean(teamId) && isReady }
}

export function useAppVersions(appId: string | null | undefined): SyncedAppVersion[] {
  const { data } = useLiveQuery(
    (query) =>
      appId ? query.from({ v: appVersionCollection }).where(({ v }) => eq(v.appId, appId)) : undefined,
    [appId]
  )
  return useMemo(
    () => [...((data ?? []) as SyncedAppVersion[])].sort((a, b) => b.number - a.number),
    [data]
  )
}

/** The caller's editor session ROOT for the app (null = none yet). */
export function useEditorSession(
  teamId: string | null | undefined,
  appId: string | null | undefined,
  me: string | null | undefined
): { root: SessionRow | null; teamSessions: readonly SessionRow[] } {
  const { rows } = useTeamSessions(teamId)
  const root = useMemo(
    () =>
      appId && me
        ? (rows.find((row) => row.appId === appId && row.userId === me && row.parentId === null) ?? null)
        : null,
    [rows, appId, me]
  )
  return { root, teamSessions: rows }
}

/** #VAPP-186: an app's name by id (a comment's "via <app>"), null while
 *  unknown, trashed or not synced. */
export function useAppName(appId: string | null | undefined): string | null {
  const { data } = useLiveQuery(
    (query) => (appId ? query.from({ a: appCollection }).where(({ a }) => and(eq(a.id, appId), isNull(a.deletedAt))) : undefined),
    [appId]
  )
  return ((data ?? []) as App[])[0]?.name ?? null
}
