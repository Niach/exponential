import { useState } from "react"
import { createFileRoute, redirect, useNavigate, useRouter } from "@tanstack/react-router"
import { ciDefaultJob, ciFixPrompt } from "@exp/domain-contract/ci-run"
import { conceptIcon, EmptyState, MobileDetailHeader, SkeletonRows, useIsMobile } from "@exp/ui"
import { LINK_NO_ACCESS } from "@/lib/app-link"
import { CiRunView, useRunClock } from "@/components/ci/ci-run-page"
import { pageFrameClass } from "@/components/page-frame"
import { useCiLog, useCiRun } from "@/hooks/use-ci-run"
import { failedAction } from "@/lib/action-error"
import { pageTitle } from "@/lib/page-title"
import { trpc } from "@/lib/trpc-client"
import { ciRunCollection } from "@/lib/collections"
import { ciRunHeader, type CiRunBusy } from "@/lib/ci-run-view"
import { PushedTitle } from "@/components/team/mobile-page-header"

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json): THE CI run page, the target
// of every check's `details_url` our CI writes. `?job=<jobId>` = the
// selected job (`ciDefaultJob` when absent or unknown); selecting rewrites it.

export const Route = createFileRoute(`/t/$teamSlug/ci/$runId`)({
  head: () => ({ meta: [{ title: pageTitle(`CI run`) }] }),
  validateSearch: (search: Record<string, unknown>): { job?: string; open?: 1 } => ({
    ...(typeof search.job === `string` ? { job: search.job } : {}),
    // #EXP-2172: a phone drilled into the job (Back returns to the run).
    ...(search.open === 1 || search.open === `1` ? { open: 1 as const } : {}),
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: CiRunRoute,
})

function CiRunRoute() {
  const { teamSlug, runId } = Route.useParams()
  const { job, open } = Route.useSearch()
  const phone = useIsMobile()
  const navigate = useNavigate()
  const { model, ready } = useCiRun(runId)
  const selected = model ? ciDefaultJob(model.jobs, job) : null
  const log = useCiLog(selected)
  const now = useRunClock(model)
  const [busy, setBusy] = useState<CiRunBusy>(null)

  // Wide: a press selects (replace); a phone drills in (a new entry, so
  // Back returns to the run).
  const selectJob = (jobId: string) =>
    phone
      ? void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug, runId }, search: { job: jobId, open: 1 } })
      : void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug, runId }, search: { job: jobId }, replace: true })

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

  const router = useRouter()
  const header = model ? ciRunHeader(model) : null
  return (
    <div className="h-full overflow-y-auto max-md:h-dvh">
      {/* #EXP-2172: phones put `name #n` / the repository in the header. */}
      {phone && (
        <div className="sticky top-0 z-20 pt-[env(safe-area-inset-top)]">
          <MobileDetailHeader
            title={
              <PushedTitle>
                {header?.title ?? ``}
                {header?.subtitle ? <span className="font-normal text-muted-foreground"> · {header.subtitle}</span> : null}
              </PushedTitle>
            }
            onBack={() => (router.history.canGoBack() ? router.history.back() : void navigate({ to: `/ci` }))}
          />
        </div>
      )}
      <div className={pageFrameClass(true)}>
        {model ? (
          <CiRunView
            model={model}
            requestedJob={job}
            log={log.loaded && log.jobId === selected ? log.text : null}
            now={now}
            busy={busy}
            jobOpen={open === 1}
            onSelectJob={selectJob}
            onRerun={() => void rerun(true)}
            onRerunAll={() => void rerun(false)}
            onCancel={() => void cancel()}
            onFix={fix}
            onOpenExternal={() => model.run.externalUrl && window.open(model.run.externalUrl, `_blank`, `noopener`)}
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
