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
  RUNNER_TIMEOUT_OPTIONS,
  ciJobRows,
  ciLogView,
  ciRunActions,
  ciRunTiles,
  ciSteps,
  normalizeRunnerLabel,
  runnerNotice,
  runnerOsLabel,
  type CiJobRow,
  type CiStepLike,
  type RunnerClient,
} from "@exp/domain-contract/ci-run"
import { isFailingCheck } from "@exp/domain-contract/pr-checks"

// #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, fixtures/runner-settings.json):
// the CI run page and a device's CI runner section as genUI trees (the core
// catalog's StatTile, Section, Row, Collapsible, Trace, CodeBlock, Alert,
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
  }
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
export const CI_RUN_EVENTS = { select: `ci-select-job`, rerun: `ci-rerun`, cancel: `ci-cancel` } as const

/** A job row's leading glyph, tinted by its tone. */
function glyphNode(id: string, row: Pick<CiJobRow, `glyph` | `tone`>): NestedNode {
  return { id, component: `Icon`, props: { name: row.glyph, tone: ICON_TONE[row.tone], size: `sm` } }
}

/** One job's detail: its steps as a Trace, then its log. */
function jobDetail(
  model: CiRunModel,
  jobId: string,
  log: string | null,
  now: Date | string,
  phone: boolean
): NestedNode {
  const job = model.jobs.find((j) => j.id === jobId)!
  const steps = ciSteps(job.steps, now)
  const view = ciLogView(log ?? ``)
  const running = job.status !== `completed`
  const children: NestedNode[] = []
  if (steps.length > 0) {
    children.push({
      id: `${CI_RUN_TEST_IDS.steps}.${job.id}`,
      component: `Trace`,
      children: steps.map((step, i) => ({
        id: `${CI_RUN_TEST_IDS.steps}.${job.id}.${i}`,
        component: `TraceItem`,
        props: { title: step.title, status: step.status, ...(step.description ? { description: step.description } : {}) },
      })),
    })
  }
  if (view.code === `` && job.status !== `in_progress`) {
    children.push({ id: `${CI_RUN_TEST_IDS.log}.${job.id}`, component: `Text`, props: { text: CI_RUN_COPY.emptyLog, variant: `muted` } })
  } else {
    children.push({
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
        variant: isFailingCheck(job) ? `destructive` : `default`,
        highlight: view.highlight,
        follow: running,
      },
    })
  }
  return { id: `ci-run-detail.${job.id}`, component: `Stack`, props: { direction: `vertical`, gap: `md`, align: `stretch` }, children }
}

/** The run page (ci-run.json LAYOUT). `log` = the SELECTED job's log text
 *  as read so far (null = not read yet). */
export function ciRunTree(
  model: CiRunModel,
  options: { selected: string | null; log: string | null; now: Date | string; phone: boolean; busy?: `rerun` | `cancel` | null }
): NestedNode {
  const { selected, log, now, phone } = options
  const { run } = model
  const tiles = ciRunTiles(run, model.jobs, now)
  const actions = ciRunActions(run)
  const children: NestedNode[] = [
    {
      id: `ci-run-title`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `xs`, align: `stretch` },
      children: [
        { id: `ci-run-heading`, component: `Heading`, props: { text: CI_RUN_COPY.title.replace(`{number}`, String(run.number)), level: `h2` } },
        { id: `ci-run-name`, component: `Text`, props: { text: run.name, variant: `muted` } },
      ],
    },
    {
      id: CI_RUN_TEST_IDS.tiles,
      component: `Grid`,
      props: { columns: phone ? 2 : 4, gap: `md` },
      children: tiles.map((tile, i) => ({
        id: `${CI_RUN_TEST_IDS.tiles}.${i}`,
        component: `StatTile`,
        props: { label: tile.label, value: tile.value, tone: tile.tone },
      })),
    },
  ]
  if (actions.rerunFailed || actions.cancel) {
    const buttons: NestedNode[] = []
    if (actions.rerunFailed) {
      buttons.push({
        id: CI_RUN_TEST_IDS.rerun,
        component: `Button`,
        props: { label: CI_RUN_COPY.rerunFailed, loading: options.busy === `rerun`, disabled: Boolean(options.busy) },
        on: { press: { event: { name: CI_RUN_EVENTS.rerun } } },
      })
    }
    if (actions.cancel) {
      buttons.push({
        id: CI_RUN_TEST_IDS.cancel,
        component: `Button`,
        // ci-run.json: variant destructive-outline (the catalog's
        // `destructive`; it has no outline twin).
        props: { label: CI_RUN_COPY.cancel, variant: `destructive`, loading: options.busy === `cancel`, disabled: Boolean(options.busy) },
        on: { press: { event: { name: CI_RUN_EVENTS.cancel } } },
      })
    }
    children.push({ id: `ci-run-actions`, component: `Stack`, props: { direction: `horizontal`, gap: `sm`, align: `center` }, children: buttons })
  }
  if (run.error) {
    children.push({ id: `ci-run-error`, component: `Alert`, props: { type: `error`, title: CI_RUN_COPY.pipelineError, message: run.error } })
  } else if (model.jobs.length > 0) {
    const rows = ciJobRows(model.jobs, now)
    if (phone) {
      children.push({
        id: `ci-run-jobs`,
        component: `Stack`,
        props: { direction: `vertical`, gap: `sm`, align: `stretch` },
        children: rows.map((row) => ({
          id: `${CI_RUN_TEST_IDS.jobRow}.${row.id}`,
          component: `Collapsible`,
          props: { title: row.title, open: row.id === selected },
          on: { change: { event: { name: CI_RUN_EVENTS.select, context: { job: row.id } } } },
          children: [jobDetail(model, row.id, row.id === selected ? log : null, now, phone)],
        })),
      })
    } else {
      children.push({
        id: `ci-run-jobs`,
        component: `Box`,
        style: { display: `grid`, gridTemplateColumns: `300px minmax(0, 1fr)`, gap: `$spacing.lg`, alignItems: `start` },
        children: [
          {
            id: `ci-run-jobs-list`,
            component: `Box`,
            style: { flexDirection: `column` },
            children: [
              {
                id: `ci-run-jobs-section`,
                component: `Section`,
                props: { title: CI_RUN_COPY.jobs },
                children: rows.map((row) => ({
                  id: `${CI_RUN_TEST_IDS.jobRow}.${row.id}`,
                  component: `Row`,
                  props: {
                    title: row.title,
                    ...(row.subtitle ? { subtitle: row.subtitle } : {}),
                    ...(row.duration ? { meta: row.duration } : {}),
                    selected: row.id === selected,
                    pressable: true,
                  },
                  slots: { leading: glyphNode(`${CI_RUN_TEST_IDS.jobRow}.${row.id}.glyph`, row) },
                  on: { press: { event: { name: CI_RUN_EVENTS.select, context: { job: row.id } } } },
                })),
              },
            ],
          },
          ...(selected
            ? [{ id: `ci-run-detail`, component: `Box`, style: { minWidth: 0, flexDirection: `column` }, children: [jobDetail(model, selected, log, now, phone)] } satisfies NestedNode]
            : []),
        ],
      })
    }
  }
  return { id: CI_RUN_TEST_IDS.page, component: `Stack`, props: { direction: `vertical`, gap: `lg`, align: `stretch` }, children }
}

// ── the CI runner section (runner-settings.json) ─────────────────────────

export interface RunnerSettings {
  enabled: boolean
  labels: string[]
  slots: number
  timeoutMin: number
}

/** The row's settings, `defaults` filling a null `runner`. */
export function runnerSettingsOf(runner: Partial<RunnerSettings> | null | undefined): RunnerSettings {
  return {
    enabled: runner?.enabled ?? RUNNER_DEFAULTS.enabled,
    labels: runner?.labels ?? [...RUNNER_DEFAULTS.labels],
    slots: runner?.slots ?? RUNNER_DEFAULTS.slots,
    timeoutMin: runner?.timeoutMin ?? RUNNER_DEFAULTS.timeoutMin,
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

export const RUNNER_EVENTS = { enabled: `runner-enabled`, labels: `runner-labels`, slots: `runner-slots`, timeout: `runner-timeout` } as const

/** The section (runner-settings.json LAYOUT). */
export function runnerSectionTree(input: {
  client: RunnerClient
  caps: readonly string[]
  platform: string | null | undefined
  settings: RunnerSettings
  owner: boolean
  refused: boolean
  recent: CiJobRow[]
}): NestedNode {
  const copy = RUNNER_SETTINGS_COPY
  const ids = RUNNER_SETTINGS_TEST_IDS
  const disabled = !input.owner
  const notice = runnerNotice(input.client, input.caps)
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
        props: { label: ``, name: ids.switch, checked: input.settings.enabled, disabled },
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
