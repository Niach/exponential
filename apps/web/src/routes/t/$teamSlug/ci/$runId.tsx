import { useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { ciDefaultJob } from "@exp/domain-contract/ci-run"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { LINK_NO_ACCESS } from "@/lib/app-link"
import { CiRunView, useRunClock } from "@/components/ci/ci-run-page"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useCiLog, useCiRun } from "@/hooks/use-ci-run"
import { failedAction } from "@/lib/action-error"
import { pageTitle } from "@/lib/page-title"
import { trpc } from "@/lib/trpc-client"

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json): THE CI run page, the target
// of every check's `details_url` our CI writes. `?job=<jobId>` = the
// selected job (`ciDefaultJob` when absent or unknown); selecting rewrites it.

export const Route = createFileRoute(`/t/$teamSlug/ci/$runId`)({
  head: () => ({ meta: [{ title: pageTitle(`CI run`) }] }),
  validateSearch: (search: Record<string, unknown>): { job?: string } =>
    typeof search.job === `string` ? { job: search.job } : {},
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: CiRunRoute,
})

function CiRunRoute() {
  const { teamSlug, runId } = Route.useParams()
  const { job } = Route.useSearch()
  const navigate = useNavigate()
  const { model, ready } = useCiRun(runId)
  const selected = model ? ciDefaultJob(model.jobs, job) : null
  const log = useCiLog(selected)
  const now = useRunClock(model)
  const [busy, setBusy] = useState<`rerun` | `cancel` | null>(null)

  const selectJob = (jobId: string) =>
    void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug, runId }, search: { job: jobId }, replace: true })

  const rerun = async () => {
    setBusy(`rerun`)
    try {
      const { runId: next } = await trpc.ci.rerun.mutate({ runId, failedOnly: true })
      void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug, runId: next }, search: {} })
    } catch (error) {
      failedAction(`re-run`, `the failed jobs`, error)
    } finally {
      setBusy(null)
    }
  }

  const cancel = async () => {
    setBusy(`cancel`)
    try {
      await trpc.ci.cancel.mutate({ runId })
    } catch (error) {
      failedAction(`cancel`, `the run`, error)
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-6xl md:py-6 ${TAB_BAR_CLEARANCE}`}>
        {model ? (
          <CiRunView
            model={model}
            requestedJob={job}
            log={log.loaded && log.jobId === selected ? log.text : null}
            now={now}
            busy={busy}
            onSelectJob={selectJob}
            onRerun={() => void rerun()}
            onCancel={() => void cancel()}
          />
        ) : ready ? (
          <EmptyState icon={conceptIcon(`nav-search`)} title={LINK_NO_ACCESS} size="page" />
        ) : (
          <SkeletonRows rowHeight={60} />
        )}
      </div>
    </div>
  )
}
