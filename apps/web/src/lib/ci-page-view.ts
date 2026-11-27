import type { NestedNode } from "@exponential_app/eui"
import {
  CI_PAGE_COPY,
  CI_PAGE_TEST_IDS,
  CI_STATUS_TONES,
  ciRepoOptions,
  ciRunGroups,
  ciRunnerChip,
  ciRunnerRows,
  ciTabCounts,
  ciTrainGroups,
  type CiPageInput,
  type CiRunFilter,
  type CiRunListRow,
  type CiTab,
  type CiTrainGroup,
} from "@exp/domain-contract/ci-page"

// #EXP-2172 (fixtures/ci-page.json, pinned by fixtures/ci-page-view.json;
// the approved redesign of session 64abb994): THE CI page as ONE genUI tree,
// painted by `<ExponentialSurface>` here and by the desktop, iOS and Android
// renderers from the same builder (each mirrors this file node for node).
// Every rule comes from `@exp/domain-contract/ci-page`; this file only lays
// the page out. The platform adds its own page chrome (title, back).

/** Events the page's tree fires. Context keys: `run`, `pr`, `device`; the
 *  pickers and tabs carry the control's own `value`. */
export const CI_PAGE_EVENTS = {
  tab: `ci-tab`,
  status: `ci-filter-status`,
  repository: `ci-filter-repo`,
  source: `ci-filter-source`,
  openRun: `ci-open-run`,
  openPr: `ci-open-pr`,
  openRunner: `ci-open-runner`,
  runners: `ci-open-runners`,
} as const

export interface CiPageState {
  tab: CiTab
  filter: CiRunFilter
  phone: boolean
}

const TRAIN_TONE = { "ci-running": CI_STATUS_TONES.running.tone, "ci-queued": `neutral`, "ci-skipped": `neutral`, "ci-failed": `danger`, "ci-passed": `success` } as const

function trainSection(group: CiTrainGroup, id: string): NestedNode {
  return {
    id,
    component: `Section`,
    props: { title: group.title, caption: group.caption, icon: `pr-batch` },
    children: group.rows.map((row) => ({
      id: `${CI_PAGE_TEST_IDS.trainRow}.${row.id}`,
      component: `Row`,
      props: { identifier: row.identifier, title: row.title, ...(row.subtitle ? { subtitle: row.subtitle } : {}), pressable: true },
      slots: {
        leading: { id: `${CI_PAGE_TEST_IDS.trainRow}.${row.id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: TRAIN_TONE[row.glyph], size: `sm` } },
        trailing: { id: `${CI_PAGE_TEST_IDS.trainRow}.${row.id}.badge`, component: `Badge`, props: { text: row.badge, variant: `secondary` } },
      },
      on: { press: { event: { name: CI_PAGE_EVENTS.openPr, context: { pr: row.pullRequestId } } } },
    })),
  }
}

function filters(input: CiPageInput, state: CiPageState): NestedNode {
  const copy = CI_PAGE_COPY
  const status: NestedNode = {
    id: CI_PAGE_TEST_IDS.statusFilter,
    component: `Segmented`,
    props: {
      items: ([`all`, `running`, `failed`, `mine`] as const).map((value) => ({ label: copy.filters[value], value })),
      value: state.filter.status,
      size: `sm`,
      ...(state.phone ? { fill: true } : {}),
    },
    on: { change: { event: { name: CI_PAGE_EVENTS.status } } },
  }
  const repository: NestedNode = {
    id: CI_PAGE_TEST_IDS.repoFilter,
    component: `Select`,
    props: { name: CI_PAGE_TEST_IDS.repoFilter, triggerVariant: `pill`, value: state.filter.repositoryId ?? `all`, options: ciRepoOptions(input) },
    on: { change: { event: { name: CI_PAGE_EVENTS.repository } } },
  }
  const source: NestedNode = {
    id: CI_PAGE_TEST_IDS.sourceFilter,
    component: `Select`,
    props: {
      name: CI_PAGE_TEST_IDS.sourceFilter,
      triggerVariant: `pill`,
      value: state.filter.source ?? `all`,
      options: [
        { label: copy.sources.all, value: `all` },
        { label: copy.sources.exponential, value: `exponential` },
        { label: copy.sources.github, value: `github` },
      ],
    },
    on: { change: { event: { name: CI_PAGE_EVENTS.source } } },
  }
  const chip = ciRunnerChip(input)
  const runners: NestedNode | null = chip
    ? {
        id: CI_PAGE_TEST_IDS.runnerChip,
        component: `Chip`,
        props: { label: chip.label, dot: chip.online ? `$color.success` : `$color.mutedForeground`, pressable: true },
        on: { press: { event: { name: CI_PAGE_EVENTS.runners } } },
      }
    : null
  if (state.phone) {
    return {
      id: CI_PAGE_TEST_IDS.filters,
      component: `Stack`,
      props: { direction: `vertical`, gap: `sm`, align: `stretch` },
      children: [status, { id: `${CI_PAGE_TEST_IDS.filters}.picks`, component: `Toolbar`, children: [repository, source] }],
    }
  }
  return {
    id: CI_PAGE_TEST_IDS.filters,
    component: `Toolbar`,
    children: [status, repository, source],
    ...(runners ? { slots: { trailing: runners } } : {}),
  }
}

/** One run's row. Wide: title, subtitle, trailing duration · age, the
 *  source mark (tooltip) and the author; phones: meta = duration | queued. */
function runRow(row: CiRunListRow, phone: boolean): NestedNode {
  const id = `${CI_PAGE_TEST_IDS.runRow}.${row.id}`
  const leading: NestedNode = { id: `${id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: row.tone, size: `sm` } }
  const press = { press: { event: { name: CI_PAGE_EVENTS.openRun, context: { run: row.id } } } }
  if (phone) {
    return { id, component: `Row`, props: { title: row.title, subtitle: row.phoneSubtitle, meta: row.phoneMeta, pressable: true }, slots: { leading }, on: press }
  }
  const age = { "@call": `formatRelativeTime`, args: { value: row.at } }
  const meta = row.duration
    ? { "@call": `fill`, args: { template: `{duration} · {age}`, params: { duration: row.duration, age } } }
    : { "@call": `fill`, args: { template: `{queued} · {age}`, params: { queued: CI_PAGE_COPY.waitingForRunner, age } } }
  const trailing: NestedNode[] = [
    { id: `${id}.meta`, component: `Text`, props: { text: meta, variant: `muted` } },
    {
      id: `${id}.source`,
      component: `Tooltip`,
      props: { content: row.sourceLabel },
      children: [{ id: `${id}.source.icon`, component: `Icon`, props: { name: row.sourceIcon, size: `sm`, tone: `neutral`, label: row.sourceLabel } }],
    },
  ]
  if (row.author) {
    trailing.push({
      id: `${id}.author`,
      component: `Avatar`,
      props: { name: row.author.name, ...(row.author.image ? { src: row.author.image } : {}), seed: row.author.id, size: `sm` },
    })
  }
  return {
    id,
    component: `Row`,
    props: { title: row.title, subtitle: row.subtitle, pressable: true },
    slots: {
      leading,
      trailing: { id: `${id}.trailing`, component: `Stack`, props: { direction: `horizontal`, gap: `sm`, align: `center` }, children: trailing },
    },
    on: press,
  }
}

function runsPanel(input: CiPageInput, state: CiPageState): NestedNode {
  const copy = CI_PAGE_COPY
  const groups = ciRunGroups(input, state.filter)
  const children: NestedNode[] = [filters(input, state)]
  const filtered = state.filter.status !== `all` || state.filter.repositoryId !== null || state.filter.source !== null
  if (groups.length === 0) {
    children.push({
      id: CI_PAGE_TEST_IDS.runs,
      component: `EmptyState`,
      props: { icon: `nav-ci`, title: filtered ? copy.empty.filtered : copy.empty.runs, size: `list` },
    })
  } else {
    children.push({
      id: CI_PAGE_TEST_IDS.runs,
      component: `Stack`,
      props: { direction: `vertical`, gap: `md`, align: `stretch` },
      children: groups.map((group) => ({
        id: `${CI_PAGE_TEST_IDS.runs}.${group.id}`,
        component: `Section`,
        props: { title: group.title, ...(group.count !== null ? { count: group.count } : {}) },
        children: group.rows.map((row) => runRow(row, state.phone)),
      })),
    })
  }
  return { id: `${CI_PAGE_TEST_IDS.page}.runs`, component: `Stack`, props: { direction: `vertical`, gap: `md`, align: `stretch` }, children }
}

function trainPanel(input: CiPageInput): NestedNode {
  const groups = ciTrainGroups(input, true)
  return {
    id: `${CI_PAGE_TEST_IDS.page}.train`,
    component: `Stack`,
    props: { direction: `vertical`, gap: `md`, align: `stretch` },
    children:
      groups.length === 0
        ? [{ id: CI_PAGE_TEST_IDS.train, component: `EmptyState`, props: { icon: `pr-batch`, title: CI_PAGE_COPY.empty.train, size: `list` } }]
        : groups.map((group) => trainSection(group, `${CI_PAGE_TEST_IDS.train}.${group.repositoryId}`)),
  }
}

function runnersPanel(input: CiPageInput): NestedNode {
  const copy = CI_PAGE_COPY.runners
  const rows = ciRunnerRows(input)
  if (rows.length === 0) {
    return { id: CI_PAGE_TEST_IDS.runners, component: `EmptyState`, props: { icon: `server`, title: CI_PAGE_COPY.empty.runners, size: `list` } }
  }
  return {
    id: CI_PAGE_TEST_IDS.runners,
    component: `Section`,
    props: {},
    children: rows.map((row) => {
      const meta = row.busy
        ? row.busy
        : row.offlineSince
          ? { "@call": `fill`, args: { template: copy.offlineSince, params: { time: { "@call": `formatRelativeTime`, args: { value: row.offlineSince } } } } }
          : null
      return {
        id: `${CI_PAGE_TEST_IDS.runnerRow}.${row.id}`,
        component: `Row`,
        props: { icon: `server`, title: row.title, subtitle: row.subtitle, ...(meta ? { meta } : {}), pressable: true },
        ...(row.queue
          ? { slots: { trailing: { id: `${CI_PAGE_TEST_IDS.runnerRow}.${row.id}.queue`, component: `Badge`, props: { text: row.queue, variant: `outline` } } } }
          : {}),
        on: { press: { event: { name: CI_PAGE_EVENTS.openRunner, context: { device: row.id } } } },
      } satisfies NestedNode
    }),
  }
}

/** The CI page (ci-page.json LAYOUT): the tab strip, then the current tab's
 *  panel (the others are empty stacks). */
export function ciPageTree(input: CiPageInput, state: CiPageState): NestedNode {
  const copy = CI_PAGE_COPY
  const counts = ciTabCounts(input)
  const empty = (tab: CiTab): NestedNode => ({ id: `${CI_PAGE_TEST_IDS.page}.${tab}`, component: `Stack`, props: { direction: `vertical`, gap: `md`, align: `stretch` }, children: [] })
  return {
    id: CI_PAGE_TEST_IDS.page,
    component: `Stack`,
    props: { direction: `vertical`, gap: `lg`, align: `stretch` },
    children: [
      {
        id: CI_PAGE_TEST_IDS.tabs,
        component: `Tabs`,
        props: {
          tabs: [
            { label: copy.tabs.runs, value: `runs` },
            { label: copy.tabs.train, value: `train`, ...(counts.train > 0 ? { count: counts.train } : {}) },
            { label: copy.tabs.runners, value: `runners`, ...(counts.runners > 0 ? { count: counts.runners } : {}) },
          ],
          value: state.tab,
          variant: `strip`,
          ...(state.phone ? { fill: true } : {}),
        },
        on: { change: { event: { name: CI_PAGE_EVENTS.tab } } },
        children: [
          state.tab === `runs` ? runsPanel(input, state) : empty(`runs`),
          state.tab === `train` ? trainPanel(input) : empty(`train`),
          state.tab === `runners` ? runnersPanel(input) : empty(`runners`),
        ],
      },
    ],
  }
}
