import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { CI_RUN_LIVE_TAIL_MS } from "@exp/domain-contract/ci-run"
import { ciRunContext } from "@exp/domain-contract/ci-page"
import {
  ciJobCollection,
  ciRunCollection,
  deviceCollection,
  mergeTrainCollection,
  mergeTrainEventCollection,
  pullRequestCollection,
  userCollection,
} from "@/lib/collections"
import { toCiPageJob, toCiPagePr, toCiPageRun, toCiPageTrain, toCiPageTrainEvent, useCiRepos } from "@/hooks/use-ci-page"
import { useTeamRepos } from "@/hooks/use-team-repos"
import { appendLogChunk, EMPTY_LOG, type CiLogState, type CiRunModel } from "@/lib/ci-run-view"
import { trpc } from "@/lib/trpc-client"

// #EXP-1617 / #EXP-1804 (ci-run.json DATA): the run page's synced rows (the
// `ci_runs` row, its `ci_jobs`, the runner devices' labels) and the selected
// job's log, read on demand: once from 0, then from `nextOffset` every
// `liveTailMs` while not `complete`.

const toIso = (value: Date | string | null | undefined): string | null =>
  value == null ? null : typeof value === `string` ? value : value.toISOString()

/** The run as the page reads it (null = not synced / not visible). */
export function useCiRun(runId: string | null | undefined): { model: CiRunModel | null; ready: boolean } {
  const { data: runs, isReady } = useLiveQuery(
    (query) => (runId ? query.from({ r: ciRunCollection }).where(({ r }) => eq(r.id, runId)) : undefined),
    [runId]
  )
  const { data: jobs } = useLiveQuery(
    (query) => (runId ? query.from({ j: ciJobCollection }).where(({ j }) => eq(j.runId, runId)) : undefined),
    [runId]
  )
  const { data: devices } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const run0 = runs?.[0]
  const repos = useTeamRepos(run0?.teamId)
  const prId = run0?.pullRequestId ?? null
  const { data: prs } = useLiveQuery(
    (query) => (prId ? query.from({ p: pullRequestCollection }).where(({ p }) => eq(p.id, prId)) : undefined),
    [prId]
  )
  // #EXP-2214 (ci-page.json `ciRunContext`): its kind, and a merge queue
  // run's train, the train's history and the repository's PRs.
  const ciRepos = useCiRepos()
  const trainId = run0?.mergeTrainId ?? null
  const repositoryId = run0?.repositoryId ?? null
  const { data: trains } = useLiveQuery(
    (query) => (trainId ? query.from({ t: mergeTrainCollection }).where(({ t }) => eq(t.id, trainId)) : undefined),
    [trainId]
  )
  const { data: trainEvents } = useLiveQuery(
    (query) => (trainId ? query.from({ e: mergeTrainEventCollection }).where(({ e }) => eq(e.mergeTrainId, trainId)) : undefined),
    [trainId]
  )
  const { data: repoPrs } = useLiveQuery(
    (query) => (trainId && repositoryId ? query.from({ p: pullRequestCollection }).where(({ p }) => eq(p.repositoryId, repositoryId)) : undefined),
    [trainId, repositoryId]
  )
  const authorId = run0?.createdByUserId ?? null
  const { data: authors } = useLiveQuery(
    (query) => (authorId ? query.from({ u: userCollection }).where(({ u }) => eq(u.id, authorId)) : undefined),
    [authorId]
  )
  const model = useMemo<CiRunModel | null>(() => {
    const run = runs?.[0]
    if (!run) return null
    const labels = new Map((devices ?? []).map((device) => [device.id, device.label]))
    return {
      run: {
        id: run.id,
        number: run.number,
        name: run.name,
        state: run.state,
        error: run.error ?? null,
        sha: run.sha,
        startedAt: toIso(run.startedAt),
        completedAt: toIso(run.completedAt),
        createdAt: toIso(run.createdAt) ?? ``,
        title: run.title ?? null,
        ref: run.ref ?? null,
        provider: run.provider ?? `exponential`,
        externalUrl: run.externalUrl ?? null,
        externalAccess: run.externalAccess ?? null,
      },
      repository: repos?.find((repo) => repo.id === run.repositoryId)?.fullName ?? null,
      pullRequest: prs?.[0] ? { number: prs[0].number, base: prs[0].base } : null,
      author: authors?.[0]?.name ?? null,
      context: ciRunContext(toCiPageRun(run), {
        repos: ciRepos,
        pullRequests: [...(repoPrs ?? prs ?? [])].map(toCiPagePr),
        trains: (trains ?? []).map(toCiPageTrain),
        trainEvents: (trainEvents ?? []).map(toCiPageTrainEvent),
        jobs: (jobs ?? []).map(toCiPageJob),
        runs: [],
      }),
      jobs: (jobs ?? []).map((job) => ({
        id: job.id,
        check: job.check,
        position: job.position,
        status: String(job.status),
        conclusion: job.conclusion == null ? null : String(job.conclusion),
        reason: job.reason ?? null,
        runnerLabel: job.runnerDeviceId ? (labels.get(job.runnerDeviceId) ?? null) : null,
        startedAt: toIso(job.startedAt),
        completedAt: toIso(job.completedAt),
        steps: job.steps ?? [],
      })),
    }
  }, [runs, jobs, devices, repos, prs, authors, ciRepos, trains, trainEvents, repoPrs])
  return { model, ready: Boolean(isReady) }
}

/** The selected job's log, tailed live while the server says it is not
 *  complete. `enabled` false = no reads (a specimen). */
export function useCiLog(jobId: string | null, enabled = true): CiLogState {
  const [state, setState] = useState<CiLogState>(EMPTY_LOG)
  useEffect(() => {
    if (!enabled || !jobId) {
      setState(EMPTY_LOG)
      return
    }
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | null = null
    let current: CiLogState = { ...EMPTY_LOG, jobId }
    setState(current)
    const read = async () => {
      const from = current.offset
      try {
        const chunk = await trpc.ci.log.query({ jobId, offset: from })
        if (stopped) return
        current = appendLogChunk(current, jobId, from, chunk)
        setState(current)
      } catch (error) {
        if (stopped) return
        console.warn(`[ci] log read failed`, error)
      }
      if (!stopped && !current.complete) timer = setTimeout(() => void read(), CI_RUN_LIVE_TAIL_MS)
    }
    void read()
    return () => {
      stopped = true
      if (timer) clearTimeout(timer)
    }
  }, [jobId, enabled])
  return state
}
