import type { NestedNode } from "@exponential_app/eui"
import type { CheckTone } from "@exp/domain-contract/pr-checks"
import {
  CI_RUN_COPY,
  CI_RUN_LOG_MAX_LINES,
  CI_RUN_TEST_IDS,
  RUNNER_DEFAULTS,
  RUNNER_LIMITS,
  RUNNER_SETTINGS_COPY,
  RUNNER_SETTINGS_TEST_IDS,
  RUNNER_MEMORY_OPTIONS,
  RUNNER_TIMEOUT_OPTIONS,
  ciDefaultJob,
  ciFailureSummary,
  ciJobRows,
  ciJobsCaption,
  ciJobSummary,
  ciLogView,
  ciRunActions,
  ciRunChips,
  ciSteps,
  normalizeRunnerLabel,
  runnerNotice,
  runnerOsLabel,
  runnerUnsupported,
  type CiJobRow,
  type RunnerRecentRow,
  type CiStepLike,
  type RunnerClient,
} from "@exp/domain-contract/ci-run"

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, fixtures/runner-settings.json),
// redesigned by #EXP-2172 (session 64abb994): the CI run page and a device's
// CI runner section as genUI trees (the core catalog's Breadcrumb, Heading,
// Chip, Toolbar, Button, Menu, Alert, Section, Row, Card, Trace, CodeBlock;
// SettingsRow, Switch, ChipInput, NumberField, Select), painted by
// `<ExponentialSurface>` so the web draws exactly what the catalogue draws
// ×4. Every rule comes from `@exp/domain-contract/ci-run`; this file only
// lays the fixture's LAYOUT out. Node ids = the fixtures' `testIds`
// (`data-xui-id`); a per-job part appends `.<jobId>`.

const ICON_TONE: Record<CheckTone, `neutral` | `success` | `danger`> = {
  muted: `neutral`,
  green: `success`,
  red: `danger`,
}

/** The run page's input: the synced rows, already plain (ISO strings). */
export interface CiRunModel {
  run: {
    id: string
    number: number
    name: string
    state: string
    error: string | null
    sha: string
    startedAt: string | null
    completedAt: string | null
    createdAt: string
    /** #EXP-2172: what the run is about (else the pipeline's name). */
    title: string | null
    ref: string | null
    provider: string
    externalUrl: string | null
    externalAccess: string | null
  }
  /** The repository's full name (null while its row has not synced). */
  repository: string | null
  /** The run's pull request, when known. */
  pullRequest: { number: number; base: string } | null
  /** The author's name (the users row of createdByUserId), when known. */
  author: string | null
  jobs: Array<{
    id: string
    check: string
    position: number
    status: string
    conclusion: string | null
    reason: string | null
    runnerLabel: string | null
    startedAt: string | null
    completedAt: string | null
    steps: CiStepLike[]
  }>
}

/** Events the page's tree fires (`onAction` names). */
export const CI_RUN_EVENTS = {
  select: `ci-select-job`,
  rerun: `ci-rerun`,
  rerunAll: `ci-rerun-all`,
  cancel: `ci-cancel`,
  fix: `ci-fix`,
  openExternal: `ci-open-external`,
  menu: `ci-run-menu`,
} as const

/** A busy action (its button spins, the others wait). */
export type CiRunBusy = `rerun` | `rerunAll` | `cancel` | null

/** The run's title (ci-page.json's row rule, without the train batch). */
export function ciRunTitle(model: CiRunModel): string {
  return model.run.title ?? model.run.name
}

/** The platform header on phones: `name #n` over the repository. */
export function ciRunHeader(model: CiRunModel): { title: string; subtitle: string | null } {
  return { title: `${model.run.name} #${model.run.number}`, subtitle: model.repository }
}

/** One job's Card: its summary line, the steps with the open step holding
 *  the log (or the log alone when the job reported no steps). */
function jobCard(model: CiRunModel, jobId: string, log: string | null, now: Date | string, phone: boolean): NestedNode {
  const job = model.jobs.find((j) => j.id === jobId)!
  const steps = ciSteps(job.steps, now)
  const view = ciLogView(log ?? ``)
  const running = job.status !== `completed`
  const logNode: NestedNode =
    view.code === `` && job.status !== `in_progress`
      ? { id: `${CI_RUN_TEST_IDS.log}.${job.id}`, component: `Text`, props: { text: CI_RUN_COPY.emptyLog, variant: `muted` } }
      : {
          id: `${CI_RUN_TEST_IDS.log}.${job.id}`,
          component: `CodeBlock`,
          props: {
            code: view.code,
            title: CI_RUN_COPY.log,
            language: `ansi`,
            lineNumbers: true,
            wrap: false,
            copyable: true,
            maxLines: phone ? CI_RUN_LOG_MAX_LINES.phone : CI_RUN_LOG_MAX_LINES.wide,
            highlight: view.highlight,
            follow: running,
          },
        }
  const open = steps.findIndex((step) => step.status === `error`)
  const openAt = open >= 0 ? open : steps.findIndex((step) => step.status === `running`) >= 0 ? steps.findIndex((step) => step.status === `running`) : steps.length - 1
  const children: NestedNode[] =
    steps.length === 0
      ? [logNode]
      : [
          {
            id: `${CI_RUN_TEST_IDS.steps}.${job.id}`,
            component: `Trace`,
            props: { size: `sm` },
            children: steps.map((step, i) => ({
              id: `${CI_RUN_TEST_IDS.steps}.${job.id}.${i}`,
              component: `TraceItem`,
              props: {
                title: step.title,
                status: step.status,
                ...(step.description ? { description: step.description } : {}),
                collapsible: true,
                open: i === openAt,
              },
              ...(i === openAt ? { children: [logNode] } : {}),
            })),
          },
        ]
  return {
    id: `${CI_RUN_TEST_IDS.jobCard}.${job.id}`,
    component: `Card`,
    props: { title: job.check, description: ciJobSummary(job, now) },
    children,
  }
}

/** The run page (ci-run.json LAYOUT). `log` = the SELECTED job's log as read
 *  so far (null = not read yet). Phones: `jobOpen` = drilled into the
 *  selected job (the tree is its Card alone). */
export function ciRunTree(
  model: CiRunModel,
  options: { selected: string | null; log: string | null; now: Date | string; phone: boolean; busy?: CiRunBusy; jobOpen?: boolean }
): NestedNode {
  const { log, now, phone } = options
  const { run } = model
  const selected = options.selected ?? ciDefaultJob(model.jobs, null)
  if (phone && options.jobOpen && selected) {
    return { id: CI_RUN_TEST_IDS.page, component: `Stack`, props: { direction: `vertical`, gap: `lg`, align: `stretch` }, children: [jobCard(model, selected, log, now, phone)] }
  }
  const head: NestedNode[] = []
  head.push({ id: `ci-run-heading`, component: `Heading`, props: { text: ciRunTitle(model), level: phone ? `h3` : `h2` } })
  if (!phone) {
    const age = { "@call": `formatRelativeTime`, args: { value: run.startedAt ?? run.createdAt } }
    head.push({
      id: `ci-run-name`,
      component: `Text`,
      props: {
        text: { "@call": `fill`, args: { template: `{workflow} · {repository}{age}`, params: { workflow: `${run.name} #${run.number}`, repository: model.repository ?? ``, age: { "@call": `fill`, args: { template: ` · {t}`, params: { t: age } } } } } },
        variant: `muted`,
      },
    })
  }
  const chips: NestedNode = {
    id: CI_RUN_TEST_IDS.chips,
    component: `Stack`,
    props: { direction: `horizontal`, gap: `xs`, align: `center`, wrap: true },
    children: ciRunChips({ run, jobs: model.jobs, pullRequest: model.pullRequest, author: model.author, now }).map((chip) => ({
      id: `${CI_RUN_TEST_IDS.chips}.${chip.id}`,
      component: `Chip`,
      props: { label: chip.label, ...(chip.icon ? { icon: chip.icon } : {}), ...(chip.dot ? { dot: chip.dot } : {}) },
    })),
  }
  const actions = ciRunActions(run)
  const button = (id: string, label: string, event: string, busy: CiRunBusy, extra: Record<string, unknown> = {}): NestedNode => ({
    id,
    component: `Button`,
    props: { label, size: `sm`, ...extra, ...(busy ? { loading: options.busy === busy } : {}), disabled: Boolean(options.busy) },
    on: { press: { event: { name: event } } },
  })
  const buttons: NestedNode[] = []
  if (actions.primary === `cancel`) buttons.push(button(CI_RUN_TEST_IDS.cancel, CI_RUN_COPY.cancel, CI_RUN_EVENTS.cancel, `cancel`, { icon: `circle-stop` }))
  if (actions.primary === `rerunFailed`) buttons.push(button(CI_RUN_TEST_IDS.rerun, CI_RUN_COPY.rerunFailed, CI_RUN_EVENTS.rerun, `rerun`, { icon: `refresh-cw` }))
  if (actions.primary === `rerunAll`) buttons.push(button(CI_RUN_TEST_IDS.rerunAll, CI_RUN_COPY.rerunAll, CI_RUN_EVENTS.rerunAll, `rerunAll`, { icon: `refresh-cw` }))
  if (actions.fix) buttons.push(button(CI_RUN_TEST_IDS.fix, CI_RUN_COPY.fix, CI_RUN_EVENTS.fix, null, { variant: `outline`, icon: `bot` }))
  if (actions.menu.length > 0) {
    buttons.push({
      id: CI_RUN_TEST_IDS.more,
      component: `Menu`,
      props: {
        label: CI_RUN_COPY.more,
        align: `end`,
        items: actions.menu.map((item) => ({ label: item === `rerunAll` ? CI_RUN_COPY.rerunAll : CI_RUN_COPY.openExternal, value: item })),
      },
      on: { select: { event: { name: CI_RUN_EVENTS.menu } } },
      children: [
        { id: `${CI_RUN_TEST_IDS.more}.button`, component: `Button`, props: { icon: `ellipsis`, variant: `ghost`, size: `icon`, label: CI_RUN_COPY.more, disabled: Boolean(options.busy) } },
      ],
    })
  }
  const actionsNode: NestedNode = { id: `ci-run-actions`, component: `Stack`, props: { direction: `horizontal`, gap: `sm`, align: `center` }, children: buttons }
  const children: NestedNode[] = [
    { id: `ci-run-title`, component: `Stack`, props: { direction: `vertical`, gap: `xs`, align: `stretch` }, children: head },
    phone
      ? { id: `ci-run-bar`, component: `Stack`, props: { direction: `vertical`, gap: `sm`, align: `stretch` }, children: [chips, ...(buttons.length > 0 ? [actionsNode] : [])] }
      : { id: `ci-run-bar`, component: `Toolbar`, children: [chips], ...(buttons.length > 0 ? { slots: { trailing: actionsNode } } : {}) },
  ]
  if (run.error) {
    children.push({ id: `ci-run-error`, component: `Alert`, props: { type: `error`, title: CI_RUN_COPY.pipelineError, message: run.error } })
    return { id: CI_RUN_TEST_IDS.page, component: `Stack`, props: { direction: `vertical`, gap: `lg`, align: `stretch` }, children }
  }
  const failedJob = model.jobs.find((job) => job.id === selected && ciJobRows([job], now)[0]?.status === `failed`) ?? null
  const failure = ciFailureSummary(failedJob, failedJob ? log : null)
  if (failure) children.push({ id: CI_RUN_TEST_IDS.failure, component: `Alert`, props: { type: `error`, title: failure.title, message: failure.message } })
  if (model.jobs.length === 0) return { id: CI_RUN_TEST_IDS.page, component: `Stack`, props: { direction: `vertical`, gap: `lg`, align: `stretch` }, children }
  const rows = ciJobRows(model.jobs, now)
  const section: NestedNode = {
    id: `ci-run-jobs-section`,
    component: `Section`,
    props: { title: CI_RUN_COPY.jobs, caption: ciJobsCaption(model.jobs) },
    children: rows.map((row: CiJobRow) => ({
      id: `${CI_RUN_TEST_IDS.jobRow}.${row.id}`,
      component: `Row`,
      props: {
        title: row.title,
        ...(row.subtitle && !phone ? { subtitle: row.subtitle } : {}),
        ...(row.duration ? { meta: row.duration } : row.subtitle && phone ? { meta: row.subtitle } : {}),
        selected: row.id === selected,
        pressable: true,
        ...(phone ? { chevron: `right` } : {}),
      },
      slots: { leading: { id: `${CI_RUN_TEST_IDS.jobRow}.${row.id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: row.statusTone, size: `sm` } } },
      on: { press: { event: { name: CI_RUN_EVENTS.select, context: { job: row.id } } } },
    })),
  }
  if (phone) {
    children.push(section)
  } else {
    children.push({
      id: `ci-run-jobs`,
      component: `Box`,
      style: { display: `grid`, gridTemplateColumns: `300px minmax(0, 1fr)`, gap: `$spacing.lg`, alignItems: `start` },
      children: [
        { id: `ci-run-jobs-list`, component: `Box`, style: { flexDirection: `column` }, children: [section] },
        ...(selected
          ? [{ id: `ci-run-detail`, component: `Box`, style: { minWidth: 0, flexDirection: `column` }, children: [jobCard(model, selected, log, now, phone)] } satisfies NestedNode]
          : []),
      ],
    })
  }
  return { id: CI_RUN_TEST_IDS.page, component: `Stack`, props: { direction: `vertical`, gap: `lg`, align: `stretch` }, children }
}

/** A job row's leading glyph for the runner section (its own tones). */
function glyphNode(id: string, row: Pick<CiJobRow, `glyph` | `tone`>): NestedNode {
  return { id, component: `Icon`, props: { name: row.glyph, tone: ICON_TONE[row.tone], size: `sm` } }
}

// ── the CI runner section (runner-settings.json) ─────────────────────────

export interface RunnerSettings {
  enabled: boolean
  labels: string[]
  slots: number
  timeoutMin: number
  /** #EXP-2193: the job's memory cap; pause jobs while a game runs. */
  memoryMaxMb: number
  pauseWhenBusy: boolean
}

/** The row's settings, `defaults` filling a null `runner` (and the fields an older row lacks). */
export function runnerSettingsOf(runner: Partial<RunnerSettings> | null | undefined): RunnerSettings {
  return {
    enabled: runner?.enabled ?? RUNNER_DEFAULTS.enabled,
    labels: runner?.labels ?? [...RUNNER_DEFAULTS.labels],
    slots: runner?.slots ?? RUNNER_DEFAULTS.slots,
    timeoutMin: runner?.timeoutMin ?? RUNNER_DEFAULTS.timeoutMin,
    memoryMaxMb: runner?.memoryMaxMb ?? RUNNER_DEFAULTS.memoryMaxMb,
    pauseWhenBusy: runner?.pauseWhenBusy ?? RUNNER_DEFAULTS.pauseWhenBusy,
  }
}

/** A ChipInput's entries as stored: each through `normalizeRunnerLabel`,
 *  duplicates dropped, at most `limits.labels`; `refused` = an entry the
 *  rule turned down (shown inline). */
export function runnerLabelsFrom(entries: readonly string[]): { labels: string[]; refused: boolean } {
  const labels: string[] = []
  let refused = false
  for (const entry of entries) {
    const label = normalizeRunnerLabel(entry)
    if (label === null) refused = true
    else if (!labels.includes(label) && labels.length < RUNNER_LIMITS.labels) labels.push(label)
  }
  return { labels, refused }
}

/** The pointer the ChipInput's `values` is bound to (the host writes the
 *  normalized list back, so a refused entry leaves the field). */
export const RUNNER_LABELS_PATH = `/labels`

export const RUNNER_EVENTS = {
  enabled: `runner-enabled`,
  labels: `runner-labels`,
  slots: `runner-slots`,
  timeout: `runner-timeout`,
  memory: `runner-memory`,
  pause: `runner-pause`,
} as const

/** The section (runner-settings.json LAYOUT). */
export function runnerSectionTree(input: {
  client: RunnerClient
  caps: readonly string[]
  kind: string | null | undefined
  platform: string | null | undefined
  settings: RunnerSettings
  owner: boolean
  refused: boolean
  recent: RunnerRecentRow[]
}): NestedNode {
  const copy = RUNNER_SETTINGS_COPY
  const ids = RUNNER_SETTINGS_TEST_IDS
  const disabled = !input.owner
  const notice = runnerNotice(input.client, input.caps, input.kind, input.platform)
  const switchDisabled = disabled || (runnerUnsupported(input.kind, input.platform) && !input.settings.enabled)
  const os = runnerOsLabel(input.platform)
  const children: NestedNode[] = []
  if (notice) {
    const n = copy.notice[notice]
    children.push({ id: `runner-notice`, component: `Alert`, props: { type: n.type, title: n.title, message: n.message } })
  }
  children.push({
    id: `runner-enabled-row`,
    component: `SettingsRow`,
    props: { label: copy.enabled.label, description: copy.enabled.description },
    children: [
      {
        id: ids.switch,
        component: `Switch`,
        props: { label: ``, name: ids.switch, checked: input.settings.enabled, disabled: switchDisabled },
        on: { change: { event: { name: RUNNER_EVENTS.enabled } } },
      },
    ],
  })
  if (input.settings.enabled) {
    children.push({
      id: ids.labels,
      component: `ChipInput`,
      props: {
        label: copy.labels.label,
        name: ids.labels,
        values: { "@path": RUNNER_LABELS_PATH },
        placeholder: copy.labels.placeholder,
        max: RUNNER_LIMITS.labels,
        suggestions: os ? [{ label: os, value: os }] : [],
        disabled,
      },
      on: { change: { event: { name: RUNNER_EVENTS.labels } } },
    })
    if (input.refused) {
      children.push({ id: `runner-labels-invalid`, component: `Text`, props: { text: copy.labels.invalid, variant: `caption`, tone: `danger` } })
    }
    children.push(
      {
        id: ids.slots,
        component: `NumberField`,
        props: {
          label: copy.slots,
          name: ids.slots,
          value: input.settings.slots,
          min: RUNNER_LIMITS.slots.min,
          max: RUNNER_LIMITS.slots.max,
          step: 1,
          disabled,
        },
        on: { change: { event: { name: RUNNER_EVENTS.slots } } },
      },
      {
        id: ids.timeout,
        component: `Select`,
        props: {
          label: copy.timeout,
          name: ids.timeout,
          options: RUNNER_TIMEOUT_OPTIONS,
          value: String(input.settings.timeoutMin),
          disabled,
        },
        on: { change: { event: { name: RUNNER_EVENTS.timeout } } },
      },
      {
        id: ids.memory,
        component: `Select`,
        props: {
          label: copy.memory,
          name: ids.memory,
          options: RUNNER_MEMORY_OPTIONS,
          value: String(input.settings.memoryMaxMb),
          disabled,
        },
        on: { change: { event: { name: RUNNER_EVENTS.memory } } },
      },
      {
        id: `runner-pause-row`,
        component: `SettingsRow`,
        props: { label: copy.pause },
        children: [
          {
            id: ids.pause,
            component: `Switch`,
            props: { label: ``, name: ids.pause, checked: input.settings.pauseWhenBusy, disabled },
            on: { change: { event: { name: RUNNER_EVENTS.pause } } },
          },
        ],
      }
    )
  }
  children.push({ id: `runner-recent-heading`, component: `Heading`, props: { text: copy.recent, level: `h4` } })
  children.push(
    input.recent.length > 0
      ? {
          id: ids.recent,
          component: `Stack`,
          props: { direction: `vertical`, gap: `none`, align: `stretch` },
          children: input.recent.map((row) => ({
            id: `${ids.recent}.${row.id}`,
            component: `Row`,
            props: { title: row.title, ...(row.subtitle ? { subtitle: row.subtitle } : {}), ...(row.duration ? { meta: row.duration } : {}) },
            slots: { leading: glyphNode(`${ids.recent}.${row.id}.glyph`, row) },
          })),
        }
      : { id: ids.recent, component: `Text`, props: { text: copy.noJobs, variant: `muted` } }
  )
  return { id: ids.section, component: `Section`, props: { title: copy.section }, children }
}

/** The settings a change event writes (null = nothing changed / refused). */
export function runnerSettingsAfter(
  current: RunnerSettings,
  event: string,
  payload: Record<string, unknown> | undefined
): { settings: RunnerSettings | null; refused: boolean } {
  const p = payload ?? {}
  if (event === RUNNER_EVENTS.enabled && typeof p.checked === `boolean`) {
    return { settings: p.checked === current.enabled ? null : { ...current, enabled: p.checked }, refused: false }
  }
  if (event === RUNNER_EVENTS.labels && Array.isArray(p.values)) {
    const { labels, refused } = runnerLabelsFrom(p.values.map(String))
    const same = labels.length === current.labels.length && labels.every((l, i) => l === current.labels[i])
    return { settings: same ? null : { ...current, labels }, refused }
  }
  if (event === RUNNER_EVENTS.slots && typeof p.value === `number` && Number.isFinite(p.value)) {
    const slots = Math.min(RUNNER_LIMITS.slots.max, Math.max(RUNNER_LIMITS.slots.min, Math.round(p.value)))
    return { settings: slots === current.slots ? null : { ...current, slots }, refused: false }
  }
  if (event === RUNNER_EVENTS.timeout && typeof p.value === `string`) {
    const timeoutMin = Number(p.value)
    const known = RUNNER_TIMEOUT_OPTIONS.some((o) => o.value === p.value)
    return { settings: !known || timeoutMin === current.timeoutMin ? null : { ...current, timeoutMin }, refused: false }
  }
  if (event === RUNNER_EVENTS.memory && typeof p.value === `string`) {
    const memoryMaxMb = Number(p.value)
    const known = RUNNER_MEMORY_OPTIONS.some((o) => o.value === p.value)
    return { settings: !known || memoryMaxMb === current.memoryMaxMb ? null : { ...current, memoryMaxMb }, refused: false }
  }
  if (event === RUNNER_EVENTS.pause && typeof p.checked === `boolean`) {
    return { settings: p.checked === current.pauseWhenBusy ? null : { ...current, pauseWhenBusy: p.checked }, refused: false }
  }
  return { settings: null, refused: false }
}

// ── the live log tail (ci-run.json: read from 0, then from nextOffset) ───

export interface CiLogState {
  jobId: string | null
  text: string
  offset: number
  complete: boolean
  loaded: boolean
}

export const EMPTY_LOG: CiLogState = { jobId: null, text: ``, offset: 0, complete: false, loaded: false }

/** One `ci.log` read appended; a chunk for another job or an older offset
 *  (a late reply) is dropped. */
export function appendLogChunk(
  state: CiLogState,
  jobId: string,
  readFrom: number,
  chunk: { text: string; nextOffset: number; complete: boolean }
): CiLogState {
  if (state.jobId !== jobId || readFrom !== state.offset) return state
  return { jobId, text: state.text + chunk.text, offset: chunk.nextOffset, complete: chunk.complete, loaded: true }
}
