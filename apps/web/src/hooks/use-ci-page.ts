import { useEffect, useMemo, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { ciBadge, type CiPageInput, type CiPageRepo } from "@exp/domain-contract/ci-page"
import { ciJobCollection, ciRunCollection, deviceCollection, pullRequestCollection, sessionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// #EXP-2172 (ci-page.json DATA): THE CI page's input, every member team at
// once, all from the synced shapes (runs, jobs, pull requests, sessions,
// devices). Repository names come from `repositories.list` per team until the
// repositories shape (#EXP-2171) is synced.

const toIso = (value: Date | string | null | undefined): string | null =>
  value == null ? null : typeof value === `string` ? value : value.toISOString()

/** `repositories.list` of every member team, one read per team. */
export function useCiRepos(teamIds: readonly string[]): CiPageRepo[] {
  const key = teamIds.join(`,`)
  const [repos, setRepos] = useState<CiPageRepo[]>([])
  useEffect(() => {
    if (teamIds.length === 0) return
    let active = true
    void Promise.all(
      teamIds.map((teamId) =>
        trpc.repositories.list
          .query({ teamId })
          .then((rows) => rows.map((row) => ({ id: row.id, fullName: row.fullName })))
          .catch(() => [] as CiPageRepo[])
      )
    ).then((lists) => {
      if (active) setRepos(lists.flat())
    })
    return () => {
      active = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])
  return repos
}

/** The page's input, without the clock and the repositories. */
export function useCiPageRows(viewerId: string): Omit<CiPageInput, `now` | `repos`> & { ready: boolean } {
  const { data: runs, isReady } = useLiveQuery((query) => query.from({ r: ciRunCollection }), [])
  const { data: jobs } = useLiveQuery((query) => query.from({ j: ciJobCollection }), [])
  const { data: prs } = useLiveQuery((query) => query.from({ p: pullRequestCollection }), [])
  const { data: sessions } = useLiveQuery((query) => query.from({ s: sessionCollection }), [])
  const { data: devices } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  return useMemo(
    () => ({
      ready: Boolean(isReady),
      viewerId,
      runs: (runs ?? []).map((run) => ({
        id: run.id,
        repositoryId: run.repositoryId,
        number: run.number,
        name: run.name,
        sha: run.sha,
        ref: run.ref ?? null,
        trigger: run.trigger,
        state: run.state,
        pullRequestId: run.pullRequestId ?? null,
        createdByUserId: run.createdByUserId ?? null,
        provider: run.provider ?? `exponential`,
        startedAt: toIso(run.startedAt),
        completedAt: toIso(run.completedAt),
        createdAt: toIso(run.createdAt) ?? ``,
      })),
      jobs: (jobs ?? []).map((job) => ({
        runId: job.runId,
        status: String(job.status),
        conclusion: job.conclusion == null ? null : String(job.conclusion),
        runsOn: job.runsOn ?? [],
        runnerDeviceId: job.runnerDeviceId ?? null,
        provider: job.provider ?? `exponential`,
      })),
      pullRequests: (prs ?? []).map((pr) => ({
        id: pr.id,
        number: pr.number,
        title: pr.title,
        repositoryId: pr.repositoryId,
        rootSessionId: pr.rootSessionId ?? null,
        base: pr.base,
        mergeState: pr.mergeState ?? null,
        mergePosition: pr.mergePosition ?? null,
        mergeTrainId: pr.mergeTrainId ?? null,
      })),
      sessions: (sessions ?? []).filter((s) => s.rootId === s.id).map((s) => ({ id: s.id, userId: s.userId })),
      runners: (devices ?? [])
        .filter((device) => device.runner?.enabled === true && !device.spaceHostId)
        .map((device) => ({
          id: device.id,
          label: device.label,
          platform: device.platform ?? null,
          labels: device.runner?.labels ?? [],
          slots: device.runner?.slots ?? 1,
          lastSeenAt: toIso(device.lastSeenAt),
        })),
    }),
    [isReady, viewerId, runs, jobs, prs, sessions, devices]
  )
}

/** nav-badges.json destination `ci` (the sidebar's CI row). */
export function useCiBadge(viewerId: string): { count: number; tone: `attention` | `muted` } {
  const rows = useCiPageRows(viewerId)
  return useMemo(() => ciBadge(rows), [rows])
}
