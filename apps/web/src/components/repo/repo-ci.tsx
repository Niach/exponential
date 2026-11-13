import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { SurfaceActionEvent, SurfaceState } from "@exponential_app/eui-react"
import { REPO_PAGE_COPY } from "@exp/domain-contract/repo-page"
import { REPO_CI_TEST_IDS, pipelineRows, requiredSuggestions, runRows } from "@exp/domain-contract/repo-ci"
import { EmptyState, SkeletonRows, useIsMobile } from "@exp/ui"
import { CiSurface } from "@/components/ci/ci-surface"
import { useNow } from "@/hooks/use-now"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { ciJobCollection, ciRunCollection } from "@/lib/collections"
import {
  REPO_CI_EVENTS,
  REPO_CI_PATHS,
  repoCiConfigAfter,
  repoCiSectionTree,
  repoRunInput,
  repoRunsTree,
  secretFromSubmit,
  type RepoCiSecret,
} from "@/lib/repo-ci-view"
import { trpc } from "@/lib/trpc-client"

// #EXP-1617 E4 part 4 (fixtures/repo-ci.json): the repo page's Runs tab and
// the CI section of its Settings tab, over the synced `ci_runs` / `ci_jobs`
// of the repository and `ci.repoSettings`.

/** The repository's synced runs (reference inputs) and their jobs. */
function useRepoRuns(repositoryId: string) {
  const { data: runs, isReady } = useLiveQuery(
    (q) => q.from({ r: ciRunCollection }).where(({ r }) => eq(r.repositoryId, repositoryId)),
    [repositoryId]
  )
  const runIds = useMemo(() => (runs ?? []).map((run) => run.id).sort(), [runs])
  const { data: jobs } = useLiveQuery(
    (q) => (runIds.length > 0 ? q.from({ j: ciJobCollection }).where(({ j }) => inArray(j.runId, runIds)) : undefined),
    [runIds.join(`,`)]
  )
  return useMemo(
    () => ({
      ready: Boolean(isReady),
      runs: (runs ?? []).map(repoRunInput),
      jobs: (jobs ?? []).map((job) => ({
        runId: job.runId,
        check: job.check,
        status: String(job.status),
        conclusion: job.conclusion == null ? null : String(job.conclusion),
      })),
    }),
    [isReady, runs, jobs]
  )
}

export function RepoRunsTab({ repositoryId, teamSlug }: { repositoryId: string; teamSlug: string }) {
  const navigate = useNavigate()
  const phone = useIsMobile()
  const now = useNow(30_000)
  const { ready, runs, jobs } = useRepoRuns(repositoryId)
  const rows = useMemo(() => runRows(runs, jobs, now), [runs, jobs, now])
  const tree = useMemo(() => repoRunsTree(rows, phone), [rows, phone])
  if (!ready && rows.length === 0) return <SkeletonRows rowHeight={52} />
  if (rows.length === 0) return <EmptyState size="list" title={REPO_PAGE_COPY.runs.empty} />
  const onAction = (event: SurfaceActionEvent) => {
    const runId = event.context.run
    if (event.name === REPO_CI_EVENTS.openRun && typeof runId === `string`) {
      void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug, runId } })
    }
  }
  return (
    <div data-testid={REPO_CI_TEST_IDS.runs} className="w-full min-w-0">
      <CiSurface surfaceId={`repo-runs-${repositoryId}`} tree={tree} onAction={onAction} />
    </div>
  )
}

type CiConfig = { requiredChecks: string[]; githubStatuses: boolean; actionsRunners: boolean }

const configOf = (raw: { requiredChecks?: string[]; githubStatuses?: boolean; actionsRunners?: boolean } | null | undefined): CiConfig => ({
  requiredChecks: raw?.requiredChecks ?? [],
  githubStatuses: raw?.githubStatuses ?? false,
  actionsRunners: raw?.actionsRunners ?? false,
})

const iso = (value: Date | string): string => (typeof value === `string` ? value : value.toISOString())

/** The CI section of the Settings tab. `manager` = the member who shared
 *  the repo or a team owner (everyone else: disabled, no secret actions). */
export function RepoCiSection({ repositoryId, provider, manager }: { repositoryId: string; provider: string; manager: boolean }) {
  const phone = useIsMobile()
  const { runs, jobs } = useRepoRuns(repositoryId)
  const [config, setConfig] = useState<CiConfig | null>(null)
  const [secrets, setSecrets] = useState<RepoCiSecret[] | null>(null)
  const [busy, setBusy] = useState(false)
  const surfaceRef = useRef<SurfaceState | null>(null)
  const deleting = useRef<string | null>(null)

  const load = useCallback(async () => {
    try {
      const next = await trpc.ci.repoSettings.query({ repositoryId }, SKIP_ERROR_TOAST)
      setConfig(configOf(next.config))
      setSecrets(next.secrets.map((s) => ({ name: s.name, updatedAt: iso(s.updatedAt) })))
    } catch (err) {
      setConfig((c) => c ?? configOf(null))
      setSecrets((s) => s ?? [])
      failedAction(`load`, `the CI settings`, err)
    }
  }, [repositoryId])
  useEffect(() => {
    void load()
  }, [load])

  const current = config ?? configOf(null)
  const pipelines = useMemo(() => pipelineRows(runs), [runs])
  const suggestions = useMemo(() => requiredSuggestions(runs, jobs, current.requiredChecks), [runs, jobs, current.requiredChecks])
  const tree = useMemo(
    () => repoCiSectionTree({ phone, provider, manager, pipelines, suggestions, secrets, config: current, busy }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [phone, provider, manager, pipelines, suggestions, secrets, current.githubStatuses, current.actionsRunners, busy]
  )
  // The ChipInput's `values` is bound: the stored list wins.
  const requiredKey = current.requiredChecks.join(`\n`)
  useEffect(() => {
    surfaceRef.current?.setData(REPO_CI_PATHS.required, [...current.requiredChecks])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requiredKey])

  const act = async (verb: string, thing: string, fn: () => Promise<unknown>) => {
    setBusy(true)
    try {
      await fn()
      await load()
    } catch (err) {
      failedAction(verb, thing, err, { retry: () => void act(verb, thing, fn) })
      await load()
    } finally {
      setBusy(false)
    }
  }

  const onAction = (event: SurfaceActionEvent) => {
    const set = (path: string, value: unknown) => surfaceRef.current?.setData(path, value)
    switch (event.name) {
      case REPO_CI_EVENTS.required:
      case REPO_CI_EVENTS.githubStatuses:
      case REPO_CI_EVENTS.actionsRunners: {
        const patch = repoCiConfigAfter(current, event.name, event.payload)
        if (!patch || !manager) return
        setConfig({ ...current, ...patch })
        void act(`save`, `the CI settings`, () => trpc.ci.setRepoConfig.mutate({ repositoryId, ...patch }, SKIP_ERROR_TOAST))
        return
      }
      case REPO_CI_EVENTS.saveSecret: {
        const secret = secretFromSubmit(event.payload)
        if (!secret) return
        set(REPO_CI_PATHS.secretOpen, false)
        set(REPO_CI_PATHS.secretName, ``)
        set(REPO_CI_PATHS.secretValue, ``)
        void act(`save`, `the secret`, () => trpc.ci.setSecret.mutate({ repositoryId, ...secret }, SKIP_ERROR_TOAST))
        return
      }
      case REPO_CI_EVENTS.cancelSecret:
        set(REPO_CI_PATHS.secretOpen, false)
        set(REPO_CI_PATHS.secretName, ``)
        set(REPO_CI_PATHS.secretValue, ``)
        return
      case REPO_CI_EVENTS.deleteSecret: {
        const name = event.context.name
        if (typeof name !== `string`) return
        deleting.current = name
        set(REPO_CI_PATHS.deleteOpen, true)
        return
      }
      case REPO_CI_EVENTS.confirmDelete: {
        const name = deleting.current
        deleting.current = null
        set(REPO_CI_PATHS.deleteOpen, false)
        if (name) void act(`delete`, `the secret`, () => trpc.ci.deleteSecret.mutate({ repositoryId, name }, SKIP_ERROR_TOAST))
        return
      }
      case REPO_CI_EVENTS.cancelDelete:
        deleting.current = null
        set(REPO_CI_PATHS.deleteOpen, false)
        return
    }
  }

  return (
    <div data-testid={REPO_CI_TEST_IDS.ci} className="w-full min-w-0">
      <CiSurface
        surfaceId={`repo-ci-${repositoryId}`}
        tree={tree}
        data={{ required: [...current.requiredChecks], secretOpen: false, secret: { name: ``, value: `` }, deleteOpen: false }}
        onAction={onAction}
        surfaceRef={surfaceRef}
      />
    </div>
  )
}
