import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { CI_RUN_LIVE_TAIL_MS } from "@exp/domain-contract/ci-run"
import { ciJobCollection, ciRunCollection, deviceCollection } from "@/lib/collections"
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
      },
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
  }, [runs, jobs, devices])
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
