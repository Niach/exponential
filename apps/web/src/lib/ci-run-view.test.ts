import { describe, expect, it } from "vitest"
import type { NestedNode } from "@exponential_app/eui"
import ciFixture from "@exp/domain-contract/fixtures/ci-run.json"
import runnerFixture from "@exp/domain-contract/fixtures/runner-settings.json"
import { CI_RUN_SPECIMEN, ciLogView, runnerRecentJobs } from "@exp/domain-contract/ci-run"
import {
  appendLogChunk,
  ciRunTree,
  EMPTY_LOG,
  runnerLabelsFrom,
  runnerSectionTree,
  runnerSettingsAfter,
  runnerSettingsOf,
  type CiRunModel,
} from "./ci-run-view"

const specimen = (): CiRunModel => ({
  run: { ...CI_RUN_SPECIMEN.run },
  repository: CI_RUN_SPECIMEN.repository,
  pullRequest: CI_RUN_SPECIMEN.pullRequest,
  author: CI_RUN_SPECIMEN.author,
  jobs: CI_RUN_SPECIMEN.jobs.map((job) => ({ ...job, steps: [...job.steps] })),
})

function find(node: NestedNode, id: string): NestedNode | undefined {
  if (node.id === id) return node
  for (const child of [...(node.children ?? []), ...Object.values(node.slots ?? {})]) {
    const hit = find(child, id)
    if (hit) return hit
  }
  return undefined
}

function all(node: NestedNode, component: string): NestedNode[] {
  const own = node.component === component ? [node] : []
  return [...own, ...[...(node.children ?? []), ...Object.values(node.slots ?? {})].flatMap((c) => all(c, component))]
}

const opts = { selected: `j2`, log: CI_RUN_SPECIMEN.log, now: CI_RUN_SPECIMEN.now, phone: false }

describe(`ciRunTree (ci-run.json LAYOUT, #EXP-2172 redesign)`, () => {
  it(`wide: no breadcrumb (#EXP-2169), the commit title, the chip strip, Re-run failed + Fix + ⋯, the failure, the job list beside the job Card`, () => {
    const tree = ciRunTree(specimen(), opts)
    expect(tree.id).toBe(ciFixture.testIds.page)
    expect(all(tree, `Breadcrumb`)).toHaveLength(0)
    expect(find(tree, `ci-run-heading`)?.props?.text).toBe(`Mirror prunes deleted branches`)
    expect(all(tree, `StatTile`)).toHaveLength(0)
    const chips = all(find(tree, ciFixture.testIds.chips)!, `Chip`).map((c) => c.props?.label)
    expect(chips).toEqual([`Failed`, `exp/APP-33 → main`, `8f3c2a9`, `#213`, `Mara Lind`, `12m 40s`, `mint`])
    expect(find(tree, ciFixture.testIds.rerun)?.props?.variant).toBeUndefined()
    expect(find(tree, ciFixture.testIds.fix)?.props?.variant).toBe(`outline`)
    expect(find(tree, ciFixture.testIds.more)?.props?.items).toEqual([{ label: ciFixture.copy.rerunAll, value: `rerunAll` }])
    expect(find(tree, ciFixture.testIds.failure)?.props).toMatchObject({ type: `error`, title: `Failed in test (linux)` })
    const rows = all(find(tree, `ci-run-jobs-section`)!, `Row`)
    expect(rows.map((r) => r.props?.title)).toEqual([`build`, `test (linux)`, `deploy`])
    expect(rows.map((r) => r.props?.selected)).toEqual([false, true, false])
    expect(rows[1]!.slots?.leading?.props).toMatchObject({ name: `ci-failed`, tone: `danger` })
    const card = find(tree, `${ciFixture.testIds.jobCard}.j2`)!
    expect(card.props).toMatchObject({ title: `test (linux)` })
    const steps = all(card, `TraceItem`)
    expect(steps.map((t) => [t.props?.status, t.props?.open])).toEqual([[`done`, false], [`done`, false], [`error`, true], [`skipped`, false]])
    const code = all(steps[2]!, `CodeBlock`)
    expect(code).toHaveLength(1)
    expect(code[0]!.props).toMatchObject({ language: `ansi`, lineNumbers: true, wrap: false, copyable: true, maxLines: ciFixture.logMaxLines.wide, follow: false, highlight: ciLogView(CI_RUN_SPECIMEN.log).highlight })
  })

  it(`phones: no breadcrumb, chevron rows; drilling shows the job Card alone`, () => {
    const tree = ciRunTree(specimen(), { ...opts, phone: true })
    expect(find(tree, ciFixture.testIds.breadcrumb)).toBeUndefined()
    expect(all(tree, `Row`).every((r) => r.props?.chevron === `right`)).toBe(true)
    expect(all(tree, `Card`)).toHaveLength(0)
    const drilled = ciRunTree(specimen(), { ...opts, phone: true, jobOpen: true })
    expect(drilled.children?.map((c) => c.component)).toEqual([`Card`])
    expect(find(drilled, `${ciFixture.testIds.log}.j2`)?.props?.maxLines).toBe(ciFixture.logMaxLines.phone)
  })

  it(`a pipeline error replaces the jobs with an Alert`, () => {
    const model = specimen()
    model.run.error = `/jobs/Bad: invalid_name`
    const tree = ciRunTree(model, opts)
    expect(find(tree, `ci-run-error`)?.props).toEqual({ type: `error`, title: ciFixture.copy.pipelineError, message: `/jobs/Bad: invalid_name` })
    expect(all(tree, `Row`)).toHaveLength(0)
  })

  it(`a running run: Cancel is the one button, the job's log follows, no failure summary`, () => {
    const model = specimen()
    model.run.state = `pending`
    model.run.completedAt = null
    model.jobs[1] = { ...model.jobs[1]!, status: `in_progress`, conclusion: null, completedAt: null }
    const tree = ciRunTree(model, { ...opts, log: `` })
    expect(find(tree, ciFixture.testIds.cancel)).toBeDefined()
    expect(find(tree, ciFixture.testIds.rerun)).toBeUndefined()
    expect(find(tree, ciFixture.testIds.failure)).toBeUndefined()
    expect(find(tree, `${ciFixture.testIds.log}.j2`)?.props).toMatchObject({ follow: true, code: `` })
  })

  it(`a finished job without a log says so`, () => {
    const tree = ciRunTree(specimen(), { ...opts, selected: `j3`, log: `` })
    expect(find(tree, `${ciFixture.testIds.log}.j3`)?.props).toEqual({ text: ciFixture.copy.emptyLog, variant: `muted` })
  })
})

describe(`runnerSectionTree (runner-settings.json LAYOUT)`, () => {
  const s = runnerFixture.specimen
  const sample = runnerFixture.recentJobs[0]!
  const recent = runnerRecentJobs(sample.deviceId, sample.jobs, sample.now)
  const base = {
    client: `desktop` as const,
    caps: s.device.caps,
    kind: s.device.kind,
    platform: s.device.platform,
    settings: runnerSettingsOf(s.device.runner),
    owner: true,
    refused: false,
    recent,
  }

  it(`the specimen: switch, labels with the OS suggestion, slots, timeout, five recent rows`, () => {
    const tree = runnerSectionTree(base)
    expect(tree).toMatchObject({ id: runnerFixture.testIds.section, component: `Section`, props: { title: runnerFixture.copy.section } })
    expect(find(tree, `runner-notice`)).toBeUndefined()
    expect(find(tree, runnerFixture.testIds.switch)?.props).toMatchObject({ checked: true, disabled: false })
    expect(find(tree, runnerFixture.testIds.labels)?.props).toMatchObject({ suggestions: [{ label: `linux`, value: `linux` }], max: 16 })
    expect(find(tree, runnerFixture.testIds.slots)?.props).toMatchObject({ value: 2, min: 1, max: 16, step: 1 })
    expect(find(tree, runnerFixture.testIds.timeout)?.props).toMatchObject({ value: `60`, options: runnerFixture.timeoutOptions })
    expect(find(tree, runnerFixture.testIds.recent)?.children?.map((r) => r.props?.subtitle)).toEqual(
      sample.expected.map((e) => e.subtitle)
    )
  })

  it(`disabled runner hides the fields; a phone gets the notice; a viewer is disabled; no jobs = the muted text`, () => {
    const tree = runnerSectionTree({ ...base, client: `web-mobile`, owner: false, recent: [], settings: runnerSettingsOf(null) })
    expect(find(tree, `runner-notice`)?.props).toEqual(runnerFixture.copy.notice.phone)
    expect(find(tree, runnerFixture.testIds.switch)?.props).toMatchObject({ checked: false, disabled: true })
    expect(find(tree, runnerFixture.testIds.labels)).toBeUndefined()
    expect(find(tree, runnerFixture.testIds.recent)?.props?.text).toBe(runnerFixture.copy.noJobs)
  })

  it(`an old app is told to update`, () => {
    const tree = runnerSectionTree({ ...base, client: `web`, caps: [`update-check`] })
    expect(find(tree, `runner-notice`)?.props?.title).toBe(runnerFixture.copy.notice.update.title)
    expect(find(tree, runnerFixture.testIds.memory)?.props).toMatchObject({ value: `8192`, options: runnerFixture.memoryOptions })
    expect(find(tree, runnerFixture.testIds.pause)?.props).toMatchObject({ checked: true, disabled: false })
  })

  it(`#EXP-2193: the desktop app on a Mac gets the notice and a disabled switch while off; a server daemon keeps it`, () => {
    const off = runnerSettingsOf(null)
    const mac = runnerSectionTree({ ...base, platform: `macos`, settings: off })
    expect(find(mac, `runner-notice`)?.props).toEqual(runnerFixture.copy.notice.unsupported)
    expect(find(mac, runnerFixture.testIds.switch)?.props).toMatchObject({ checked: false, disabled: true })
    const on = runnerSectionTree({ ...base, platform: `windows` })
    expect(find(on, runnerFixture.testIds.switch)?.props).toMatchObject({ checked: true, disabled: false })
    const server = runnerSectionTree({ ...base, kind: `server`, platform: `macos`, settings: off })
    expect(find(server, `runner-notice`)).toBeUndefined()
    expect(find(server, runnerFixture.testIds.switch)?.props).toMatchObject({ disabled: false })
  })
})

describe(`runner writes`, () => {
  it(`labels: normalized, duplicates dropped, a refused entry flagged`, () => {
    expect(runnerLabelsFrom([`gpu`, `  Big-Disk `, `GPU`, `two words`])).toEqual({ labels: [`gpu`, `big-disk`], refused: true })
  })

  it(`one settings object per change; nothing for a no-op`, () => {
    const current = runnerSettingsOf({ enabled: true, labels: [`gpu`], slots: 2, timeoutMin: 60 })
    expect(runnerSettingsAfter(current, `runner-enabled`, { checked: false }).settings).toEqual({ ...current, enabled: false })
    expect(runnerSettingsAfter(current, `runner-slots`, { value: 40 }).settings?.slots).toBe(16)
    expect(runnerSettingsAfter(current, `runner-timeout`, { value: `120` }).settings?.timeoutMin).toBe(120)
    expect(runnerSettingsAfter(current, `runner-timeout`, { value: `7` }).settings).toBeNull()
    expect(runnerSettingsAfter(current, `runner-labels`, { values: [`gpu`, `Bad Label`] })).toEqual({ settings: null, refused: true })
    expect(runnerSettingsAfter(current, `runner-labels`, { values: [`gpu`, `x64`] }).settings?.labels).toEqual([`gpu`, `x64`])
  })

  it(`defaults fill a null runner`, () => {
    expect(runnerSettingsOf(null)).toEqual(runnerFixture.defaults)
  })
})

describe(`appendLogChunk (the live tail)`, () => {
  it(`appends in order and drops a late or foreign reply`, () => {
    const start = { ...EMPTY_LOG, jobId: `j1` }
    const one = appendLogChunk(start, `j1`, 0, { text: `a\n`, nextOffset: 2, complete: false })
    expect(one).toEqual({ jobId: `j1`, text: `a\n`, offset: 2, complete: false, loaded: true })
    expect(appendLogChunk(one, `j1`, 0, { text: `a\n`, nextOffset: 2, complete: false })).toBe(one)
    expect(appendLogChunk(one, `j2`, 2, { text: `x`, nextOffset: 3, complete: true })).toBe(one)
    expect(appendLogChunk(one, `j1`, 2, { text: `b`, nextOffset: 3, complete: true })).toMatchObject({ text: `a\nb`, complete: true })
  })
})

describe(`the trees are valid core-catalog surfaces`, () => {
  it(`reduce without issues`, async () => {
    const { CORE_CATALOG_ID, SurfaceStore } = await import(`@exponential_app/eui`)
    const sample = runnerFixture.recentJobs[0]!
    const trees: NestedNode[] = [
      ciRunTree(specimen(), opts),
      ciRunTree(specimen(), { ...opts, phone: true }),
      runnerSectionTree({
        client: `web`,
        caps: [],
        kind: `desktop`,
        platform: `linux`,
        settings: runnerSettingsOf(runnerFixture.specimen.device.runner),
        owner: true,
        refused: true,
        recent: runnerRecentJobs(sample.deviceId, sample.jobs, sample.now),
      }),
    ]
    for (const tree of trees) {
      const store = new SurfaceStore(`t`, CORE_CATALOG_ID, () => [], { nested: tree, data: { labels: [`gpu`] } })
      expect(store.issues).toEqual([])
      expect(store.root).not.toBeNull()
    }
  })
})
