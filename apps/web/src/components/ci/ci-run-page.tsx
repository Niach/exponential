import { useMemo, useState } from "react"
import type { SurfaceActionEvent } from "@exponential_app/eui-react"
import { CI_RUN_SPECIMEN, CI_RUN_TEST_IDS, ciDefaultJob } from "@exp/domain-contract/ci-run"
import { useIsMobile } from "@exp/ui"
import { CiSurface } from "@/components/ci/ci-surface"
import { useNow } from "@/hooks/use-now"
import { CI_RUN_EVENTS, ciRunTree, type CiRunModel } from "@/lib/ci-run-view"

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
  onSelectJob,
  onRerun,
  onCancel,
}: {
  model: CiRunModel
  /** The `job` query param. */
  requestedJob: string | null | undefined
  /** The selected job's log as read so far (null = none yet). */
  log: string | null
  now: Date | string
  busy?: `rerun` | `cancel` | null
  onSelectJob: (jobId: string) => void
  onRerun: () => void
  onCancel: () => void
}) {
  const phone = useIsMobile()
  const selected = ciDefaultJob(model.jobs, requestedJob)
  const tree = useMemo(() => ciRunTree(model, { selected, log, now, phone, busy }), [model, selected, log, now, phone, busy])
  const onAction = (event: SurfaceActionEvent) => {
    if (event.name === CI_RUN_EVENTS.select) {
      const job = event.context.job
      // A phone's Collapsible: closing the open one keeps the selection.
      if (typeof job === `string` && job !== selected && event.payload?.open !== false) onSelectJob(job)
    } else if (event.name === CI_RUN_EVENTS.rerun) onRerun()
    else if (event.name === CI_RUN_EVENTS.cancel) onCancel()
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
      onCancel={() => undefined}
    />
  )
}

/** A live clock for running runs (durations tick), still for finished ones. */
export function useRunClock(model: CiRunModel | null): Date {
  const now = useNow(model?.run.state === `pending` ? 1_000 : 60_000)
  return now
}
