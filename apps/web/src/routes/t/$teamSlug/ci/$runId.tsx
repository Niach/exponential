import { useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { ciDefaultJob, ciFixPrompt } from "@exp/domain-contract/ci-run"
import { conceptIcon, EmptyState, SkeletonRows } from "@exp/ui"
import { LINK_NO_ACCESS } from "@/lib/app-link"
import { CiRunView, useRunClock } from "@/components/ci/ci-run-page"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useCiLog, useCiRun } from "@/hooks/use-ci-run"
import { failedAction } from "@/lib/action-error"
import { pageTitle } from "@/lib/page-title"
import { trpc } from "@/lib/trpc-client"
import { ciRunCollection } from "@/lib/collections"
import type { CiRunBusy } from "@/lib/ci-run-view"

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
  const [busy, setBusy] = useState<CiRunBusy>(null)

  const selectJob = (jobId: string) =>
    void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug, runId }, search: { job: jobId }, replace: true })

  // Ours: a new run opens; GitHub: the same run, a new attempt.
  const rerun = async (failedOnly: boolean) => {
    setBusy(failedOnly ? `rerun` : `rerunAll`)
    try {
      const { runId: next } = await trpc.ci.rerun.mutate({ runId, failedOnly })
      if (next !== runId) void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug, runId: next }, search: {} })
    } catch (error) {
      failedAction(`re-run`, failedOnly ? `the failed jobs` : `the run`, error)
    } finally {
      setBusy(null)
    }
  }

  // #EXP-2172: THE new-chat composer on the run's repository, its text the
  // failing job and the end of its log (ci-run.json `fixPrompt`).
  const fix = (jobId: string | null) => {
    const row = ciRunCollection.get(runId)
    if (!model || !row) return
    const job = model.jobs.find((entry) => entry.id === jobId) ?? null
    const text = ciFixPrompt({
      run: { id: model.run.id, number: model.run.number, name: model.run.name, sha: model.run.sha, ref: model.run.ref },
      repository: model.repository ?? ``,
      job: job?.check ?? null,
      log: log.loaded && log.jobId === jobId ? log.text : null,
    })
    void navigate({ to: `/home`, search: { team: row.teamId, repo: row.repositoryId, text } })
  }

  const breadcrumb = (value: string) => {
    if (value === `ci`) void navigate({ to: `/ci` })
    else if (value === `repository` && model?.repository) {
      const name = model.repository.split(`/`).pop() ?? model.repository
      void navigate({ to: `/t/$teamSlug/repos/$name`, params: { teamSlug, name } })
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
      <div className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-6xl md:py-6 ${PAGE_END_CLEARANCE}`}>
        {model ? (
          <CiRunView
            model={model}
            requestedJob={job}
            log={log.loaded && log.jobId === selected ? log.text : null}
            now={now}
            busy={busy}
            onSelectJob={selectJob}
            onRerun={() => void rerun(true)}
            onRerunAll={() => void rerun(false)}
            onCancel={() => void cancel()}
            onFix={fix}
            onOpenExternal={() => model.run.externalUrl && window.open(model.run.externalUrl, `_blank`, `noopener`)}
            onBreadcrumb={breadcrumb}
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
