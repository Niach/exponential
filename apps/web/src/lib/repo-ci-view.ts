import type { NestedNode } from "@exponential_app/eui"
import type { CheckTone } from "@exp/domain-contract/pr-checks"
import { REPO_PAGE_COPY } from "@exp/domain-contract/repo-page"
import {
  REPO_CI_COPY,
  REPO_CI_TEST_IDS,
  repoCiSwitches,
  secretName,
  type RepoRunInput,
  type RepoRunRow,
} from "@exp/domain-contract/repo-ci"

// #EXP-1617 E4 part 4 (fixtures/repo-ci.json): the repo page's Runs tab and
// the CI section of its Settings tab as genUI trees (the core catalog's
// Table, Row, Section, SettingsRow, ChipInput, Switch, Chip, Dialog/Drawer,
// Form, Input, FormActions, AlertDialog), painted by `<CiSurface>` like the
// run page and the runner section. Every rule comes from
// `@exp/domain-contract/repo-ci`; this file only lays the fixture's LAYOUT
// out. Node ids = the fixture's `testIds` (`data-xui-id`).

const C = REPO_CI_COPY.ci
const IDS = REPO_CI_TEST_IDS

const ICON_TONE: Record<CheckTone, `neutral` | `success` | `danger`> = {
  muted: `neutral`,
  green: `success`,
  red: `danger`,
}

/** repo-page.json `copy.checks[state].tone` → the theme's colour token. */
const DOT: Record<string, string> = {
  green: `$color.success`,
  amber: `$color.warning`,
  red: `$color.destructive`,
}

/** Events the trees fire (`onAction` names). */
export const REPO_CI_EVENTS = {
  openRun: `repo-run-open`,
  required: `repo-ci-required`,
  githubStatuses: `repo-ci-github-statuses`,
  actionsRunners: `repo-ci-actions-runners`,
  saveSecret: `repo-ci-save-secret`,
  cancelSecret: `repo-ci-cancel-secret`,
  deleteSecret: `repo-ci-delete-secret`,
  confirmDelete: `repo-ci-confirm-delete`,
  cancelDelete: `repo-ci-cancel-delete`,
} as const

/** The data model paths the CI section binds. */
export const REPO_CI_PATHS = {
  required: `/required`,
  secretOpen: `/secretOpen`,
  secretName: `/secret/name`,
  secretValue: `/secret/value`,
  deleteOpen: `/deleteOpen`,
} as const

/** `secretName`'s rule as the Input's regex check (≤ 128). */
export const SECRET_NAME_PATTERN = `^[A-Z_][A-Z0-9_]{0,127}$`

const toIso = (value: Date | string | null | undefined): string | null =>
  value == null ? null : typeof value === `string` ? value : value.toISOString()

/** A synced `ci_runs` row as the reference reads it. */
export function repoRunInput(run: {
  id: string
  number: number
  name: string
  sha: string
  ref?: string | null
  trigger: string
  state: string
  startedAt?: Date | string | null
  completedAt?: Date | string | null
}): RepoRunInput {
  return {
    id: run.id,
    number: run.number,
    name: run.name,
    sha: run.sha,
    ref: run.ref ?? null,
    trigger: run.trigger,
    state: run.state,
    startedAt: toIso(run.startedAt),
    completedAt: toIso(run.completedAt),
  }
}

function statusGlyph(id: string, row: Pick<RepoRunRow, `glyph` | `tone`>): NestedNode {
  return { id, component: `Icon`, props: { name: row.glyph, tone: ICON_TONE[row.tone], size: `sm` } }
}

/** The Runs tab (rows non-empty; none = the host's EmptyState). WIDE a
 *  Table, PHONES one pressable Row per run. */
export function repoRunsTree(rows: readonly RepoRunRow[], phone: boolean): NestedNode {
  const cols = REPO_CI_COPY.runs.columns
  if (phone) {
    return {
      id: IDS.runs,
      component: `Section`,
      children: rows.map((row) => ({
        id: `${IDS.runRow}.${row.id}`,
        component: `Row`,
        props: {
          identifier: row.identifier,
          title: row.title,
          subtitle: row.subtitle,
          ...(row.duration ? { meta: row.duration } : {}),
          pressable: true,
        },
        slots: { leading: statusGlyph(`${IDS.runRow}.${row.id}.glyph`, row) },
        on: { press: { event: { name: REPO_CI_EVENTS.openRun, context: { run: row.id } } } },
      })),
    }
  }
  return {
    id: IDS.runs,
    component: `Table`,
    props: {
      columns: [
        { key: `status`, label: cols.status, type: `slot`, slot: `status`, width: 128 },
        { key: `run`, label: cols.run, truncate: true },
        { key: `commit`, label: cols.commit, width: 96 },
        { key: `ref`, label: cols.ref, truncate: true },
        { key: `trigger`, label: cols.trigger, width: 128 },
        { key: `duration`, label: cols.duration, width: 104, align: `end` },
        { key: `started`, label: cols.started, type: `relativeTime`, width: 128 },
      ],
      rows: rows.map((row) => ({
        id: row.id,
        glyph: row.glyph,
        iconTone: ICON_TONE[row.tone],
        status: row.statusLabel,
        run: `${row.identifier} ${row.title}`,
        commit: row.commit,
        ref: row.ref,
        trigger: row.trigger,
        duration: row.duration ?? ``,
        started: row.startedAt,
      })),
      rowKey: `id`,
    },
    slots: {
      status: {
        id: `${IDS.runRow}-status`,
        component: `Stack`,
        props: { direction: `horizontal`, gap: `xs`, align: `center` },
        children: [
          { id: `${IDS.runRow}-status.glyph`, component: `Icon`, props: { name: { "@path": `glyph` }, tone: { "@path": `iconTone` }, size: `sm` } },
          { id: `${IDS.runRow}-status.label`, component: `Text`, props: { text: { "@path": `status` }, lines: 1 } },
        ],
      },
    },
    on: { rowPress: { event: { name: REPO_CI_EVENTS.openRun, context: { run: { "@path": `id` } } } } },
  }
}

export interface RepoCiSecret {
  name: string
  updatedAt: string
}

export interface RepoCiSectionInput {
  phone: boolean
  provider: string
  /** The member who shared the repo or a team owner. */
  manager: boolean
  pipelines: Array<{ name: string; runId: string; state: string }>
  suggestions: readonly string[]
  /** null = still loading (the Secrets block waits, the rest shows). */
  secrets: readonly RepoCiSecret[] | null
  config: { githubStatuses?: boolean; actionsRunners?: boolean }
  busy: boolean
}

function pipelinesNode(pipelines: RepoCiSectionInput[`pipelines`]): NestedNode {
  if (pipelines.length === 0) {
    return { id: IDS.pipelines, component: `Text`, props: { text: C.noPipeline, variant: `muted` } }
  }
  const checks = REPO_PAGE_COPY.checks as Record<string, { label: string; tone: string }>
  return {
    id: IDS.pipelines,
    component: `Stack`,
    props: { direction: `vertical`, gap: `none`, align: `stretch` },
    children: pipelines.map((p) => {
      const check = checks[p.state] ?? checks.pending!
      return {
        id: `${IDS.pipelines}.${p.runId}`,
        component: `Row`,
        props: { title: p.name },
        slots: {
          trailing: {
            id: `${IDS.pipelines}.${p.runId}.state`,
            component: `Chip`,
            props: { label: check.label, dot: DOT[check.tone] ?? `$color.mutedForeground` },
          },
        },
      }
    }),
  }
}

function deleteButton(id: string, name: unknown, disabled: boolean): NestedNode {
  return {
    id,
    component: `Button`,
    props: { label: C.secrets.delete, variant: `ghost`, size: `sm`, disabled },
    on: { press: { event: { name: REPO_CI_EVENTS.deleteSecret, context: { name } } } },
  }
}

function secretsNodes(input: RepoCiSectionInput): NestedNode[] {
  const { phone, manager, busy } = input
  // Still loading: the block waits (no empty Table flashing "No results").
  if (input.secrets === null) return []
  const secrets = input.secrets
  const nodes: NestedNode[] = [{ id: `repo-ci-secrets-heading`, component: `Heading`, props: { text: C.secrets.title, level: `h4` } }]
  if (secrets.length === 0) {
    nodes.push({ id: IDS.secrets, component: `Text`, props: { text: C.secrets.empty, variant: `muted` } })
  } else if (phone) {
    nodes.push({
      id: IDS.secrets,
      component: `Stack`,
      props: { direction: `vertical`, gap: `none`, align: `stretch` },
      children: secrets.map((s) => ({
        id: `${IDS.secrets}.${s.name}`,
        component: `Row`,
        props: { title: s.name, meta: { "@call": `formatRelativeTime`, args: { value: s.updatedAt } } },
        ...(manager ? { slots: { trailing: deleteButton(`${IDS.secrets}.${s.name}.delete`, s.name, busy) } } : {}),
      })),
    })
  } else {
    const cols = C.secrets.columns
    nodes.push({
      id: IDS.secrets,
      component: `Table`,
      props: {
        columns: [
          { key: `name`, label: cols.name, truncate: true },
          { key: `updatedAt`, label: cols.updated, type: `relativeTime`, width: 160 },
          ...(manager ? [{ key: `actions`, label: ``, type: `slot`, slot: `actions`, width: 104, align: `end` }] : []),
        ],
        rows: secrets.map((s) => ({ id: s.name, name: s.name, updatedAt: s.updatedAt })),
        rowKey: `id`,
      },
      ...(manager ? { slots: { actions: deleteButton(`${IDS.secrets}-delete`, { "@path": `name` }, busy) } } : {}),
    })
  }
  if (!manager) return nodes
  const form: NestedNode = {
    id: `repo-ci-secret-form`,
    component: `Form`,
    props: { name: `secret`, busy },
    on: { submit: { event: { name: REPO_CI_EVENTS.saveSecret } } },
    children: [
      {
        id: `repo-ci-secret-name`,
        component: `Input`,
        props: {
          label: C.secrets.name,
          name: `name`,
          value: { "@path": REPO_CI_PATHS.secretName },
          checks: [
            {
              condition: { "@call": `regex`, args: { value: { "@path": REPO_CI_PATHS.secretName }, pattern: SECRET_NAME_PATTERN } },
              message: C.secrets.invalid,
            },
          ],
          validateOn: `change`,
        },
      },
      {
        id: `repo-ci-secret-value`,
        component: `Input`,
        props: {
          label: C.secrets.value,
          name: `value`,
          type: `password`,
          value: { "@path": REPO_CI_PATHS.secretValue },
          checks: [{ condition: { "@call": `required`, args: { value: { "@path": REPO_CI_PATHS.secretValue } } } }],
        },
      },
      {
        id: `repo-ci-secret-actions`,
        component: `FormActions`,
        props: { submit: C.secrets.save, cancel: true },
        on: { cancel: { event: { name: REPO_CI_EVENTS.cancelSecret } } },
      },
    ],
  }
  nodes.push({
    id: `repo-ci-secret-dialog`,
    component: phone ? `Drawer` : `Dialog`,
    props: { title: C.secrets.add, open: { "@path": REPO_CI_PATHS.secretOpen } },
    slots: {
      trigger: {
        id: IDS.addSecret,
        component: `Button`,
        props: { label: C.secrets.add, variant: `outline`, size: `sm`, disabled: busy },
      },
    },
    children: [form],
  })
  nodes.push({
    id: `repo-ci-delete-secret`,
    component: `AlertDialog`,
    props: {
      title: C.secrets.deleteTitle,
      confirmLabel: C.secrets.deleteConfirm,
      cancelLabel: C.secrets.cancel,
      destructive: true,
      open: { "@path": REPO_CI_PATHS.deleteOpen },
    },
    on: {
      confirm: { event: { name: REPO_CI_EVENTS.confirmDelete } },
      cancel: { event: { name: REPO_CI_EVENTS.cancelDelete } },
    },
  })
  return nodes
}

/** The CI section of the Settings tab (repo-ci.json SETTINGS TAB). */
export function repoCiSectionTree(input: RepoCiSectionInput): NestedNode {
  const disabled = !input.manager || input.busy
  const children: NestedNode[] = [
    pipelinesNode(input.pipelines),
    {
      id: `repo-ci-required-row`,
      component: `SettingsRow`,
      props: { label: C.required.label, description: C.required.description, stackBelow: `md` },
      children: [
        {
          id: IDS.required,
          component: `ChipInput`,
          props: {
            name: IDS.required,
            values: { "@path": REPO_CI_PATHS.required },
            placeholder: C.required.placeholder,
            suggestions: input.suggestions.map((name) => ({ label: name, value: name })),
            disabled,
          },
          on: { change: { event: { name: REPO_CI_EVENTS.required } } },
        },
      ],
    },
    ...secretsNodes(input),
    ...repoCiSwitches(input.provider).map(
      (key): NestedNode => ({
        id: `${IDS[key]}-row`,
        component: `SettingsRow`,
        props: { label: C[key].label, description: C[key].description },
        children: [
          {
            id: IDS[key],
            component: `Switch`,
            props: { label: ``, name: IDS[key], checked: input.config[key] === true, disabled },
            on: { change: { event: { name: REPO_CI_EVENTS[key] } } },
          },
        ],
      })
    ),
  ]
  return { id: IDS.ci, component: `Section`, props: { title: C.section }, children }
}

/** The config patch a section event writes (null = nothing to write). */
export function repoCiConfigAfter(
  current: { requiredChecks: readonly string[]; githubStatuses: boolean; actionsRunners: boolean },
  event: string,
  payload: Record<string, unknown> | undefined
): { requiredChecks?: string[]; githubStatuses?: boolean; actionsRunners?: boolean } | null {
  const p = payload ?? {}
  if (event === REPO_CI_EVENTS.required && Array.isArray(p.values)) {
    const next: string[] = []
    for (const raw of p.values) {
      const name = String(raw).trim()
      if (name && name.length <= 255 && !next.includes(name)) next.push(name)
    }
    const same = next.length === current.requiredChecks.length && next.every((n, i) => n === current.requiredChecks[i])
    return same ? null : { requiredChecks: next.slice(0, 64) }
  }
  if (event === REPO_CI_EVENTS.githubStatuses && typeof p.checked === `boolean`) {
    return p.checked === current.githubStatuses ? null : { githubStatuses: p.checked }
  }
  if (event === REPO_CI_EVENTS.actionsRunners && typeof p.checked === `boolean`) {
    return p.checked === current.actionsRunners ? null : { actionsRunners: p.checked }
  }
  return null
}

/** The secret a Form `submit` carries (null = refused by `secretName` or empty). */
export function secretFromSubmit(payload: Record<string, unknown> | undefined): { name: string; value: string } | null {
  const values = (payload?.values ?? {}) as Record<string, unknown>
  const name = typeof values.name === `string` ? values.name.trim() : ``
  const value = typeof values.value === `string` ? values.value : ``
  if (!secretName(name) || value.length === 0) return null
  return { name, value }
}
