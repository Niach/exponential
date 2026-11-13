// #EXP-1804: what packages/domain-contract/fixtures/ci-run-view.json pins,
// computed by the web reference from each case's inputs. Its test replays
// the fixture; `scripts/bless-ci-run-view.ts` rewrites the outputs after a
// deliberate change.
import runnerFixture from "@exp/domain-contract/fixtures/runner-settings.json"
import { runnerRecentJobs, type RunnerClient } from "@exp/domain-contract/ci-run"
import { ciRunTree, runnerSectionTree, runnerSettingsAfter, type CiRunModel, type RunnerSettings } from "@/lib/ci-run-view"

type Json = Record<string, unknown>

const sample = runnerFixture.recentJobs[0]!

export function replayCiRun(c: Json): unknown {
  return ciRunTree(c.model as CiRunModel, {
    selected: (c.selected as string | null) ?? null,
    log: (c.log as string | null) ?? null,
    now: c.now as string,
    phone: c.phone === true,
    busy: (c.busy as `rerun` | `cancel` | null) ?? null,
  })
}

export function replayRunner(c: Json): unknown {
  return runnerSectionTree({
    client: c.client as RunnerClient,
    caps: c.caps as string[],
    platform: (c.platform as string | null) ?? null,
    settings: c.settings as RunnerSettings,
    owner: c.owner === true,
    refused: c.refused === true,
    recent: c.recent === true ? runnerRecentJobs(sample.deviceId, sample.jobs, sample.now) : [],
  })
}

export function replayAfter(c: Json): { settings: RunnerSettings | null; refused: boolean } {
  return runnerSettingsAfter(c.current as RunnerSettings, c.event as string, c.payload as Record<string, unknown>)
}
