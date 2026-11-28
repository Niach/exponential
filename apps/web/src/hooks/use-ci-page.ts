import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import {
  ciBadge,
  type CiPageInput,
  type CiPageJob,
  type CiPagePr,
  type CiPageRepo,
  type CiPageRun,
  type CiPageTrain,
  type CiPageTrainEvent,
} from "@exp/domain-contract/ci-page"
import type { z } from "zod"
import type {
  selectCiJobSchema,
  selectCiRunSchema,
  selectMergeTrainEventSchema,
  selectMergeTrainSchema,
  selectPullRequestSchema,
} from "@/db/schema"
import {
  ciJobCollection,
  ciRunCollection,
  deviceCollection,
  mergeTrainCollection,
  mergeTrainEventCollection,
  pullRequestCollection,
  repositoryCollection,
  sessionCollection,
  userCollection,
} from "@/lib/collections"
import { listedDevices } from "@/lib/steer-devices"

// #EXP-2172 (ci-page.json DATA): THE CI page's input, every member team at
// once, all from the synced shapes (runs, jobs, pull requests, sessions,
// devices, repositories; #EXP-2214: the merge queue's trains and history).

const toIso = (value: Date | string | null | undefined): string | null =>
  value == null ? null : typeof value === `string` ? value : value.toISOString()

// The synced rows as the CI page reads them (also the run page's context).
type Synced<S extends z.ZodTypeAny> = z.infer<S>

export const toCiPageRun = (run: Synced<typeof selectCiRunSchema>): CiPageRun => ({
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
  title: run.title ?? null,
  mergeTrainId: run.mergeTrainId ?? null,
})

export const toCiPageJob = (job: Synced<typeof selectCiJobSchema>): CiPageJob => ({
  id: job.id,
  runId: job.runId,
  check: job.check,
  status: String(job.status),
  conclusion: job.conclusion == null ? null : String(job.conclusion),
  runsOn: job.runsOn ?? [],
  runnerDeviceId: job.runnerDeviceId ?? null,
  provider: job.provider ?? `exponential`,
})

export const toCiPagePr = (pr: Synced<typeof selectPullRequestSchema>): CiPagePr => ({
  id: pr.id,
  number: pr.number,
  title: pr.title ?? ``,
  repositoryId: pr.repositoryId,
  rootSessionId: pr.rootSessionId ?? null,
  base: pr.base,
  state: String(pr.state),
  mergeState: (pr.mergeState as string | null | undefined) ?? null,
  mergeWait: pr.mergeWait ?? null,
  mergeHold: pr.mergeHold ?? null,
  mergeNow: pr.mergeNow ?? false,
  mergeAttempts: pr.mergeAttempts ?? 0,
  mergePosition: pr.mergePosition ?? null,
  mergeTrainId: pr.mergeTrainId ?? null,
})

export const toCiPageTrain = (train: Synced<typeof selectMergeTrainSchema>): CiPageTrain => ({
  id: train.id,
  repositoryId: train.repositoryId,
  base: train.base,
  baseSha: train.baseSha ?? null,
  units: train.units ?? [],
  state: train.state,
  attempt: train.attempt,
  bisectOf: train.bisectOf ?? null,
  pending: train.pending ?? [],
  untested: train.untested,
  testedSha: train.testedSha ?? null,
  createdAt: toIso(train.createdAt) ?? ``,
  finishedAt: toIso(train.finishedAt),
})

export const toCiPageTrainEvent = (e: Synced<typeof selectMergeTrainEventSchema>): CiPageTrainEvent => ({
  id: e.id,
  repositoryId: e.repositoryId,
  pullRequestId: e.pullRequestId,
  mergeTrainId: e.mergeTrainId ?? null,
  kind: e.kind,
  base: e.base,
  attempt: e.attempt,
  reason: e.reason ?? null,
  checks: e.checks ?? [],
  ciJobId: e.ciJobId ?? null,
  jobUrl: e.jobUrl ?? null,
  files: e.files ?? [],
  sha: e.sha ?? null,
  byUserId: e.byUserId ?? null,
  createdAt: toIso(e.createdAt) ?? ``,
})

/** #EXP-2214: the synced repositories (name and default branch: a push to
 *  it is a release run). */
export function useCiRepos(): CiPageRepo[] {
  const { data } = useLiveQuery((query) => query.from({ r: repositoryCollection }), [])
  return useMemo(
    () =>
      (data ?? []).map((row) => ({
        id: row.id,
        fullName: row.fullName,
        defaultBranch: row.defaultBranchOverride ?? row.defaultBranch ?? null,
      })),
    [data]
  )
}

/** The page's input, without the clock and the repositories. */
export function useCiPageRows(viewerId: string): Omit<CiPageInput, `now` | `repos` | `utcOffsetMinutes`> & { ready: boolean } {
  const { data: runs, isReady } = useLiveQuery((query) => query.from({ r: ciRunCollection }), [])
  const { data: jobs } = useLiveQuery((query) => query.from({ j: ciJobCollection }), [])
  const { data: prs } = useLiveQuery((query) => query.from({ p: pullRequestCollection }), [])
  const { data: sessions } = useLiveQuery((query) => query.from({ s: sessionCollection }), [])
  const { data: devices } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const { data: users } = useLiveQuery((query) => query.from({ u: userCollection }), [])
  const { data: trains } = useLiveQuery((query) => query.from({ t: mergeTrainCollection }), [])
  const { data: trainEvents } = useLiveQuery((query) => query.from({ e: mergeTrainEventCollection }), [])
  return useMemo(
    () => ({
      ready: Boolean(isReady),
      viewerId,
      runs: (runs ?? []).map(toCiPageRun),
      jobs: (jobs ?? []).map(toCiPageJob),
      pullRequests: (prs ?? []).map(toCiPagePr),
      trains: (trains ?? []).map(toCiPageTrain),
      trainEvents: (trainEvents ?? []).map(toCiPageTrainEvent),
      users: (users ?? []).map((user) => ({ id: user.id, name: user.name, image: user.image ?? null })),
      sessions: (sessions ?? []).filter((s) => s.rootId === s.id).map((s) => ({ id: s.id, userId: s.userId })),
      runners: listedDevices(devices ?? [])
        .filter((device) => device.runner?.enabled === true)
        .map((device) => ({
          id: device.id,
          label: device.label,
          platform: device.platform ?? null,
          labels: device.runner?.labels ?? [],
          slots: device.runner?.slots ?? 1,
          lastSeenAt: toIso(device.lastSeenAt),
        })),
    }),
    [isReady, viewerId, runs, jobs, prs, sessions, devices, users, trains, trainEvents]
  )
}

/** nav-badges.json destination `ci` (the sidebar's CI row). */
export function useCiBadge(viewerId: string): { count: number; tone: `attention` | `muted` } {
  const rows = useCiPageRows(viewerId)
  return useMemo(() => ciBadge(rows), [rows])
}
