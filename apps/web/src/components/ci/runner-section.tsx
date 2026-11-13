import { useEffect, useMemo, useRef, useState } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { SurfaceActionEvent, SurfaceState } from "@exponential_app/eui-react"
import runnerFixture from "@exp/domain-contract/fixtures/runner-settings.json"
import {
  RUNNER_SETTINGS_SPECIMEN,
  RUNNER_SETTINGS_TEST_IDS,
  runnerRecentJobs,
  type CiJobRow,
  type RunnerClient,
  type RunnerJobInput,
} from "@exp/domain-contract/ci-run"
import { useIsMobile } from "@exp/ui"
import { CiSurface } from "@/components/ci/ci-surface"
import { useNow } from "@/hooks/use-now"
import { failedAction } from "@/lib/action-error"
import {
  RUNNER_LABELS_PATH,
  runnerSectionTree,
  runnerSettingsAfter,
  runnerSettingsOf,
  type RunnerSettings,
} from "@/lib/ci-run-view"
import { ciJobCollection, ciRunCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// #EXP-1617 / #EXP-1804 (fixtures/runner-settings.json): THE CI runner
// section of a device's settings. One `ci.setRunner` per change (the whole
// settings object); every control disabled for a viewer who is not the
// device's owner.

export function RunnerSectionView({
  client,
  caps,
  platform,
  settings,
  owner,
  recent,
  onChange,
}: {
  client: RunnerClient
  caps: readonly string[]
  platform: string | null | undefined
  settings: RunnerSettings
  owner: boolean
  recent: CiJobRow[]
  /** A changed settings object (never called for a refused entry alone). */
  onChange: (next: RunnerSettings) => void
}) {
  const [refused, setRefused] = useState(false)
  const surfaceRef = useRef<SurfaceState | null>(null)
  const tree = useMemo(
    () => runnerSectionTree({ client, caps, platform, settings, owner, refused, recent }),
    [client, caps, platform, settings, owner, refused, recent]
  )
  // The ChipInput's `values` is bound: the stored labels win (a refused or
  // normalized entry is rewritten in the field).
  const labelsKey = settings.labels.join(`,`)
  useEffect(() => {
    surfaceRef.current?.setData(RUNNER_LABELS_PATH, [...settings.labels])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [labelsKey])
  const onAction = (event: SurfaceActionEvent) => {
    const { settings: next, refused: turnedDown } = runnerSettingsAfter(settings, event.name, event.payload)
    if (event.name === `runner-labels`) {
      setRefused(turnedDown)
      surfaceRef.current?.setData(RUNNER_LABELS_PATH, [...(next ?? settings).labels])
    }
    if (next) onChange(next)
  }
  return (
    <div data-testid={RUNNER_SETTINGS_TEST_IDS.section} className="w-full min-w-0">
      <CiSurface
        surfaceId="runner-settings"
        tree={tree}
        data={{ labels: [...settings.labels] }}
        onAction={onAction}
        surfaceRef={surfaceRef}
      />
    </div>
  )
}

/** The device's runner jobs with their run number and repository. */
function useRunnerJobs(deviceRowId: string | null): RunnerJobInput[] {
  const { data: jobs } = useLiveQuery(
    (query) => (deviceRowId ? query.from({ j: ciJobCollection }).where(({ j }) => eq(j.runnerDeviceId, deviceRowId)) : undefined),
    [deviceRowId]
  )
  const runIds = useMemo(() => [...new Set((jobs ?? []).map((job) => job.runId))], [jobs])
  const { data: runs } = useLiveQuery(
    (query) => (runIds.length > 0 ? query.from({ r: ciRunCollection }).where(({ r }) => inArray(r.id, runIds)) : undefined),
    [runIds.join(`,`)]
  )
  const teamIds = useMemo(() => [...new Set((runs ?? []).map((run) => run.teamId))].sort(), [runs])
  const [repoNames, setRepoNames] = useState<Map<string, string>>(new Map())
  useEffect(() => {
    let stopped = false
    void Promise.all(
      teamIds.map((teamId) => trpc.repositories.list.query({ teamId }).catch(() => [] as { id: string; fullName: string }[]))
    ).then((lists) => {
      if (!stopped) setRepoNames(new Map(lists.flat().map((repo) => [repo.id, repo.fullName])))
    })
    return () => {
      stopped = true
    }
  }, [teamIds.join(`,`)])
  return useMemo(() => {
    const byId = new Map((runs ?? []).map((run) => [run.id, run]))
    return (jobs ?? []).flatMap((job) => {
      const run = byId.get(job.runId)
      if (!run) return []
      const iso = (v: Date | string | null | undefined) => (v == null ? null : typeof v === `string` ? v : v.toISOString())
      return [
        {
          id: job.id,
          runnerDeviceId: job.runnerDeviceId ?? null,
          check: job.check,
          runNumber: run.number,
          repo: repoNames.get(run.repositoryId) ?? run.name,
          status: String(job.status),
          conclusion: job.conclusion == null ? null : String(job.conclusion),
          startedAt: iso(job.startedAt),
          completedAt: iso(job.completedAt),
        },
      ]
    })
  }, [jobs, runs, repoNames])
}

/** The live section inside Device settings (the dialog is the owner's). */
export function RunnerSection({
  device,
  owner,
}: {
  device: { id: string; deviceId: string; platform: string | null; caps: string[] | null; runner: Partial<RunnerSettings> | null }
  owner: boolean
}) {
  const phone = useIsMobile()
  const now = useNow(30_000)
  const jobs = useRunnerJobs(device.id)
  const recent = useMemo(() => runnerRecentJobs(device.id, jobs, now), [device.id, jobs, now])
  const stored = useMemo(() => runnerSettingsOf(device.runner), [device.runner])
  // The last write wins locally until the synced row catches up.
  const [pending, setPending] = useState<RunnerSettings | null>(null)
  useEffect(() => setPending(null), [stored])
  const settings = pending ?? stored
  const change = async (next: RunnerSettings) => {
    setPending(next)
    try {
      await trpc.ci.setRunner.mutate({ deviceId: device.deviceId, ...next })
    } catch (error) {
      setPending(null)
      failedAction(`save`, `the CI runner`, error)
    }
  }
  return (
    <RunnerSectionView
      client={phone ? `web-mobile` : `web`}
      caps={device.caps ?? []}
      platform={device.platform}
      settings={settings}
      owner={owner}
      recent={recent}
      onChange={(next) => void change(next)}
    />
  )
}

/** DEV-ONLY `?dev=runner-settings`: the section over the fixture's
 *  specimen and recentJobs[0] (its `now`), no server. */
export function RunnerSettingsSpecimen() {
  const s = RUNNER_SETTINGS_SPECIMEN
  const sample = runnerFixture.recentJobs[0]!
  const [settings, setSettings] = useState<RunnerSettings>(() => runnerSettingsOf(s.device.runner))
  const recent = useMemo(() => runnerRecentJobs(sample.deviceId, sample.jobs, sample.now), [sample])
  return (
    <RunnerSectionView
      client={s.client as RunnerClient}
      caps={s.device.caps}
      platform={s.device.platform}
      settings={settings}
      owner={s.owner}
      recent={recent}
      onChange={setSettings}
    />
  )
}
