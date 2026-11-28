import { useMemo, useState } from "react"
import runViewFixture from "@exp/domain-contract/fixtures/ci-run-view.json"
import { pullRequestCollection } from "@/lib/collections"
import type { SurfaceActionEvent } from "@exponential_app/eui-react"
import { CI_RUN_SPECIMEN, CI_RUN_TEST_IDS, ciDefaultJob } from "@exp/domain-contract/ci-run"
import { useIsMobile } from "@exp/ui"
import { CiSurface } from "@/components/ci/ci-surface"
import { useNow } from "@/hooks/use-now"
import { CI_RUN_EVENTS, ciRunTree, type CiRunBusy, type CiRunModel } from "@/lib/ci-run-view"

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json): THE CI run page's body over
// a run model; the route (`/t/$teamSlug/ci/$runId`) owns the data, the
// selection (`?job=`) and the mutations, the `?dev=ci-run` specimen the
// fixture's `specimen`.

export function CiRunView({
  model,
  requestedJob,
  log,
  now,
  busy = null,
  jobOpen = false,
  onSelectJob,
  onRerun,
  onRerunAll,
  onCancel,
  onFix,
  onOpenExternal,
}: {
  model: CiRunModel
  /** The `job` query param. */
  requestedJob: string | null | undefined
  /** The selected job's log as read so far (null = none yet). */
  log: string | null
  now: Date | string
  busy?: CiRunBusy
  /** Phones: drilled into the selected job (`?open=1`). */
  jobOpen?: boolean
  /** A job row's press (phones drill into it). */
  onSelectJob: (jobId: string) => void
  onRerun: () => void
  onRerunAll: () => void
  onCancel: () => void
  /** #EXP-2172: Fix with an agent (the selected job). */
  onFix: (selectedJobId: string | null) => void
  onOpenExternal: () => void
}) {
  const phone = useIsMobile()
  const selected = ciDefaultJob(model.jobs, requestedJob)
  const tree = useMemo(
    () => ciRunTree(model, { selected, log, now, phone, busy, jobOpen: phone && jobOpen }),
    [model, selected, log, now, phone, busy, jobOpen]
  )
  const onAction = (event: SurfaceActionEvent) => {
    if (event.name === CI_RUN_EVENTS.select) {
      const job = event.context.job
      if (typeof job === `string` && (phone || job !== selected)) onSelectJob(job)
    } else if (event.name === CI_RUN_EVENTS.menu) {
      const value = event.payload?.value
      if (value === `rerunAll`) onRerunAll()
      else if (value === `openExternal`) onOpenExternal()
    } else if (event.name === CI_RUN_EVENTS.rerun) onRerun()
    else if (event.name === CI_RUN_EVENTS.rerunAll) onRerunAll()
    else if (event.name === CI_RUN_EVENTS.cancel) onCancel()
    else if (event.name === CI_RUN_EVENTS.fix) onFix(selected)
    else if (event.name === CI_RUN_EVENTS.openExternal) onOpenExternal()
    else if (event.name === CI_RUN_EVENTS.openPr) {
      // #EXP-2214: a What happened row opens its pull request on the forge.
      const pr = typeof event.context.pr === `string` ? pullRequestCollection.get(event.context.pr) : undefined
      if (pr?.url) window.open(pr.url, `_blank`, `noopener`)
    }
  }
  return (
    <div data-testid={CI_RUN_TEST_IDS.page} className="w-full min-w-0">
      <CiSurface surfaceId={`ci-run-${model.run.id}`} tree={tree} onAction={onAction} />
    </div>
  )
}

/** The specimen's run as a model (ci-run.json `specimen`). */
export function ciRunSpecimenModel(): CiRunModel {
  const s = CI_RUN_SPECIMEN
  return {
    run: { ...s.run },
    repository: s.repository,
    pullRequest: s.pullRequest,
    author: s.author,
    jobs: s.jobs.map((job) => ({ ...job, steps: job.steps.map((step) => ({ ...step })) })),
  }
}

/** DEV-ONLY `?dev=ci-run`: the page over the fixture's specimen, no server. */
export function CiRunSpecimen() {
  const model = useMemo(ciRunSpecimenModel, [])
  const [job, setJob] = useState<string>(CI_RUN_SPECIMEN.selected)
  return (
    <CiRunView
      model={model}
      requestedJob={job}
      log={job === CI_RUN_SPECIMEN.selected ? CI_RUN_SPECIMEN.log : null}
      now={CI_RUN_SPECIMEN.now}
      onSelectJob={setJob}
      onRerun={() => undefined}
      onRerunAll={() => undefined}
      onCancel={() => undefined}
      onFix={() => undefined}
      onOpenExternal={() => undefined}
    />
  )
}

/** DEV-ONLY `?dev=ci-run-queue` (#EXP-2214): a merge queue run, its batch
 *  and What happened, over ci-run-view.json's queue case. */
export function CiRunQueueSpecimen() {
  const c = runViewFixture.ciRun.find((x) => x.name.startsWith(`#EXP-2214 a merge queue run:`))!
  const model = c.model as unknown as CiRunModel
  const [job, setJob] = useState<string>(c.selected as string)
  return (
    <CiRunView
      model={model}
      requestedJob={job}
      log={job === c.selected ? (c.log as string) : null}
      now={c.now as string}
      onSelectJob={setJob}
      onRerun={() => undefined}
      onRerunAll={() => undefined}
      onCancel={() => undefined}
      onFix={() => undefined}
      onOpenExternal={() => undefined}
    />
  )
}

/** A live clock for running runs (durations tick), still for finished ones. */
export function useRunClock(model: CiRunModel | null): Date {
  const now = useNow(model?.run.state === `pending` ? 1_000 : 60_000)
  return now
}
