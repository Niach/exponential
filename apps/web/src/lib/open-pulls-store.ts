import { useEffect, useMemo, useSyncExternalStore } from "react"
import { trpc } from "@/lib/trpc-client"
import type { OpenPull } from "@/lib/integrations/github-pr"

/**
 * EXP-1244: the app-wide `repositories.openPulls` store — the Reviews page AND
 * the Reviews nav dot read it (`reviewsQueue`, rule 7 + `_navDoc`), so the
 * page and its dot never disagree. One entry per team; a team whose fetch
 * fails lists nothing. The server caches 60 s too.
 */

export interface OpenPullRepo {
  teamId: string
  repositoryId: string
  fullName: string
  pulls: OpenPull[]
}

interface Entry {
  repos: OpenPullRepo[]
  fetchedAt: number
}

/** A nav mount refetches a team older than this; the page always does. */
const STALE_MS = 60_000

const entries = new Map<string, Entry>()
const inflight = new Map<string, Promise<void>>()
const listeners = new Set<() => void>()
let version = 0

function emit() {
  version += 1
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** Fetch `teamId`'s open pulls unless a fresh (or in-flight) one exists. */
export function refreshOpenPulls(
  teamId: string,
  { force = false }: { force?: boolean } = {}
): Promise<void> {
  const running = inflight.get(teamId)
  if (running) return running
  const entry = entries.get(teamId)
  if (!force && entry && Date.now() - entry.fetchedAt < STALE_MS) {
    return Promise.resolve()
  }
  const request = trpc.repositories.openPulls
    .query({ teamId })
    .then((result) => result.repos.map((repo) => ({ ...repo, teamId })))
    .catch(() => [] as OpenPullRepo[])
    .then((repos) => {
      entries.set(teamId, { repos, fetchedAt: Date.now() })
    })
    .finally(() => {
      inflight.delete(teamId)
      emit()
    })
  inflight.set(teamId, request)
  emit()
  return request
}

/** A merged pull has no Electric echo: drop it locally. */
export function removeOpenPull(repositoryId: string, prNumber: number) {
  for (const [teamId, entry] of entries) {
    entries.set(teamId, {
      ...entry,
      repos: entry.repos.map((repo) =>
        repo.repositoryId === repositoryId
          ? { ...repo, pulls: repo.pulls.filter((pull) => pull.number !== prNumber) }
          : repo
      ),
    })
  }
  emit()
}

/**
 * The store's repos for `teamIds` (in that order) + whether any is still
 * loading. `force` = refetch on mount and on a team-set change (the Reviews
 * page); otherwise only a stale team refetches (the nav dot), plus on focus.
 */
export function useOpenPulls(
  teamIds: readonly string[],
  { force = false }: { force?: boolean } = {}
): { repos: OpenPullRepo[]; loading: boolean } {
  const snapshot = useSyncExternalStore(subscribe, () => version)
  const teamKey = teamIds.join(`,`)

  useEffect(() => {
    if (teamIds.length === 0) return
    for (const teamId of teamIds) void refreshOpenPulls(teamId, { force })
    if (force) return
    const onFocus = () => {
      for (const teamId of teamIds) void refreshOpenPulls(teamId)
    }
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
    // `teamKey` names the id set.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamKey, force])

  return useMemo(
    () => ({
      repos: teamIds.flatMap((teamId) => entries.get(teamId)?.repos ?? []),
      loading: teamIds.some((teamId) => inflight.has(teamId)),
    }),
    // `snapshot` bumps on every store change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [teamKey, snapshot]
  )
}
