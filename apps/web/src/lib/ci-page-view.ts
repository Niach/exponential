import type { NestedNode } from "@exponential_app/eui"
import type { CheckTone } from "@exp/domain-contract/pr-checks"
import {
  CI_PAGE_COPY,
  CI_PAGE_TEST_IDS,
  ciPageTiles,
  ciRepoOptions,
  ciRunList,
  ciRunnerRows,
  ciTabCounts,
  ciTrainGroups,
  type CiPageInput,
  type CiRunFilter,
  type CiTab,
  type CiTrainGroup,
} from "@exp/domain-contract/ci-page"

// #EXP-2172 (fixtures/ci-page.json, pinned by fixtures/ci-page-view.json):
// THE CI page as ONE genUI tree, painted by `<ExponentialSurface>` here and
// by the desktop, iOS and Android renderers from the same builder (each
// mirrors this file node for node). Every rule comes from
// `@exp/domain-contract/ci-page`; this file only lays the page out. The
// platform adds its own page chrome (title, back) around the tree.

const ICON_TONE: Record<CheckTone, `neutral` | `success` | `danger`> = {
  muted: `neutral`,
  green: `success`,
  red: `danger`,
}

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
} as const

export interface CiPageState {
  tab: CiTab
  filter: CiRunFilter
  phone: boolean
}

function trainSection(group: CiTrainGroup, id: string): NestedNode {
  return {
    id,
    component: `Section`,
    props: { title: CI_PAGE_COPY.trainSection, caption: `${group.title} · ${group.caption}`, icon: `pr-batch` },
    children: group.rows.map((row) => ({
      id: `${CI_PAGE_TEST_IDS.trainRow}.${row.id}`,
      component: `Row`,
      props: { identifier: row.identifier, title: row.title, ...(row.subtitle ? { subtitle: row.subtitle } : {}), pressable: true },
      slots: {
        leading: { id: `${CI_PAGE_TEST_IDS.trainRow}.${row.id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: `neutral`, size: `sm` } },
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
  if (state.phone) {
    return {
      id: CI_PAGE_TEST_IDS.filters,
      component: `Stack`,
      props: { direction: `vertical`, gap: `sm`, align: `stretch` },
      children: [status, { id: `${CI_PAGE_TEST_IDS.filters}.picks`, component: `Toolbar`, children: [repository, source] }],
    }
  }
  return { id: CI_PAGE_TEST_IDS.filters, component: `Toolbar`, children: [status, repository, source] }
}

function runsPanel(input: CiPageInput, state: CiPageState): NestedNode {
  const copy = CI_PAGE_COPY
  const rows = ciRunList(input, state.filter)
  const children: NestedNode[] = [filters(input, state)]
  ciTrainGroups(input, false).forEach((group) => children.push(trainSection(group, `${CI_PAGE_TEST_IDS.train}.${group.repositoryId}`)))
  const filtered = state.filter.status !== `all` || state.filter.repositoryId !== null || state.filter.source !== null
  if (rows.length === 0) {
    children.push({
      id: CI_PAGE_TEST_IDS.runs,
      component: `EmptyState`,
      props: { icon: `workflow`, title: filtered ? copy.empty.filtered : copy.empty.runs, size: `list` },
    })
  } else if (state.phone) {
    children.push({
      id: CI_PAGE_TEST_IDS.runs,
      component: `Section`,
      props: { title: copy.runsSection },
      children: rows.map((row) => ({
        id: `${CI_PAGE_TEST_IDS.runRow}.${row.id}`,
        component: `Row`,
        props: { title: row.title, subtitle: row.subtitle, ...(row.duration ? { meta: row.duration } : {}), pressable: true },
        slots: { leading: { id: `${CI_PAGE_TEST_IDS.runRow}.${row.id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: ICON_TONE[row.tone], size: `sm` } } },
        on: { press: { event: { name: CI_PAGE_EVENTS.openRun, context: { run: row.id } } } },
      })),
    })
  } else {
    const c = copy.columns
    children.push({
      id: CI_PAGE_TEST_IDS.runs,
      component: `Table`,
      props: {
        columns: [
          { key: `status`, label: c.status, type: `slot`, slot: `status`, width: 120 },
          { key: `run`, label: c.run },
          { key: `repository`, label: c.repository },
          { key: `source`, label: c.source, type: `badge` },
          { key: `ref`, label: c.ref, truncate: true },
          { key: `trigger`, label: c.trigger },
          { key: `duration`, label: c.duration, align: `end`, width: 96 },
          { key: `started`, label: c.started, type: `relativeTime`, width: 140 },
        ],
        rows: rows.map((row) => ({
          id: row.id,
          status: row.statusLabel,
          glyph: row.glyph,
          tone: ICON_TONE[row.tone],
          run: row.title,
          repository: row.repository,
          source: row.sourceLabel,
          ref: row.ref,
          trigger: row.trigger,
          duration: row.duration ?? ``,
          started: row.startedAt ?? ``,
        })),
        rowKey: `id`,
      },
      slots: {
        status: {
          id: `${CI_PAGE_TEST_IDS.runs}.status`,
          component: `Stack`,
          props: { direction: `horizontal`, gap: `xs`, align: `center` },
          children: [
            { id: `${CI_PAGE_TEST_IDS.runs}.status.glyph`, component: `Icon`, props: { name: { "@path": `glyph` }, tone: { "@path": `tone` }, size: `sm` } },
            { id: `${CI_PAGE_TEST_IDS.runs}.status.label`, component: `Text`, props: { text: { "@path": `status` } } },
          ],
        },
      },
      on: { rowPress: { event: { name: CI_PAGE_EVENTS.openRun } } },
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

/** The CI page (ci-page.json LAYOUT): the tiles, then the tabs; only the
 *  current tab's panel is built (the others are empty stacks). */
export function ciPageTree(input: CiPageInput, state: CiPageState): NestedNode {
  const copy = CI_PAGE_COPY
  const tiles = ciPageTiles(input)
  const counts = ciTabCounts(input)
  const empty = (tab: CiTab): NestedNode => ({ id: `${CI_PAGE_TEST_IDS.page}.${tab}`, component: `Stack`, props: { direction: `vertical`, gap: `md`, align: `stretch` }, children: [] })
  return {
    id: CI_PAGE_TEST_IDS.page,
    component: `Stack`,
    props: { direction: `vertical`, gap: `lg`, align: `stretch` },
    children: [
      {
        id: CI_PAGE_TEST_IDS.tiles,
        component: `Grid`,
        props: { columns: state.phone ? 2 : 4, gap: state.phone ? `sm` : `md` },
        children: tiles.map((tile, i) => ({
          id: `${CI_PAGE_TEST_IDS.tiles}.${i}`,
          component: `StatTile`,
          props: { label: tile.label, value: tile.value, tone: tile.tone, ...(tile.delta ? { delta: tile.delta } : {}) },
        })),
      },
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
