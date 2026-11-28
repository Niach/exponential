import type { NestedNode } from "@exponential_app/eui"
import {
  CI_PAGE_COPY,
  CI_PAGE_TEST_IDS,
  ciPrGroups,
  ciQueue,
  ciRepoOptions,
  ciRunGroups,
  ciRunnerChip,
  ciRunnerRows,
  type CiPageInput,
  type CiQueueNow,
  type CiQueueRepo,
  type CiQueueRow,
  type CiRunFilter,
  type CiRunKind,
  type CiRunListRow,
  type CiSubjectGroup,
  type CiSubjectRow,
  type CiTab,
} from "@exp/domain-contract/ci-page"

// #EXP-2172 (fixtures/ci-page.json, pinned by fixtures/ci-page-view.json):
// THE CI page as ONE genUI tree, painted by `<ExponentialSurface>` here and
// by the desktop, iOS and Android renderers from the same builder (each
// mirrors this file node for node). #EXP-2214: four tabs, one meaning each:
// Merge queue, Pull requests, Releases, Runners. Every rule comes from
// `@exp/domain-contract/ci-page`; this file only lays the page out. The
// platform adds its own page chrome (title, back) and the tab switcher.

/** Events the page's tree fires. Context keys: `run`, `job`, `pr`,
 *  `device`, `url`; the pickers, tabs and menus carry the control's own
 *  `value`. */
export const CI_PAGE_EVENTS = {
  tab: `ci-tab`,
  status: `ci-filter-status`,
  repository: `ci-filter-repo`,
  source: `ci-filter-source`,
  openRun: `ci-open-run`,
  openJob: `ci-open-job`,
  openUrl: `ci-open-url`,
  openPr: `ci-open-pr`,
  openRunner: `ci-open-runner`,
  runners: `ci-open-runners`,
  /** The Up next ⋯ menu: `value` = merge-now | hold, context `pr`. */
  queueMenu: `ci-queue-menu`,
  resume: `ci-resume`,
} as const

export interface CiPageState {
  tab: CiTab
  filter: CiRunFilter
  phone: boolean
}

type Tone = `neutral` | `info` | `success` | `danger` | `warning`
const DOT: Record<Tone, string> = {
  neutral: `$color.mutedForeground`,
  info: `$color.info`,
  success: `$color.success`,
  danger: `$color.destructive`,
  warning: `$color.warning`,
}

const vstack = (id: string, children: NestedNode[], gap = `md`): NestedNode => ({
  id,
  component: `Stack`,
  props: { direction: `vertical`, gap, align: `stretch` },
  children,
})
const hstack = (id: string, children: NestedNode[]): NestedNode => ({
  id,
  component: `Stack`,
  props: { direction: `horizontal`, gap: `sm`, align: `center` },
  children,
})
const relative = (value: string) => ({ "@call": `formatRelativeTime`, args: { value } })

/** The filter bar (#EXP-2169 "one look"): ONE Toolbar of `Select{button}`
 *  pickers, status and repository (#EXP-2214: the source picker is gone);
 *  wide adds the runner Chip at its end. */
function filters(input: CiPageInput, state: CiPageState): NestedNode {
  const copy = CI_PAGE_COPY
  const picker = (id: string, label: string, value: string, options: Array<{ label: string; value: string }>, event: string): NestedNode => ({
    id,
    component: `Select`,
    props: { name: id, label, triggerVariant: `button`, value, options },
    on: { change: { event: { name: event } } },
  })
  const status = picker(
    CI_PAGE_TEST_IDS.statusFilter,
    copy.filterLabels.status,
    state.filter.status,
    ([`all`, `running`, `failed`, `mine`] as const).map((value) => ({ label: copy.filters[value], value })),
    CI_PAGE_EVENTS.status
  )
  const repository = picker(CI_PAGE_TEST_IDS.repoFilter, copy.filterLabels.repository, state.filter.repositoryId ?? `all`, ciRepoOptions(input), CI_PAGE_EVENTS.repository)
  const chip = state.phone ? null : ciRunnerChip(input)
  const runners: NestedNode | null = chip
    ? {
        id: CI_PAGE_TEST_IDS.runnerChip,
        component: `Chip`,
        props: { label: chip.label, dot: chip.online ? `$color.success` : `$color.mutedForeground`, pressable: true },
        on: { press: { event: { name: CI_PAGE_EVENTS.runners } } },
      }
    : null
  return {
    id: CI_PAGE_TEST_IDS.filters,
    component: `Toolbar`,
    children: [status, repository],
    ...(runners ? { slots: { trailing: runners } } : {}),
  }
}

const openRun = (run: string) => ({ press: { event: { name: CI_PAGE_EVENTS.openRun, context: { run } } } })
const openPr = (pr: string) => ({ press: { event: { name: CI_PAGE_EVENTS.openPr, context: { pr } } } })

/** One run's row. Wide: title, subtitle, trailing duration · age, the
 *  source mark (tooltip) and the author; phones: meta = duration | queued. */
function runRow(row: CiRunListRow, phone: boolean): NestedNode {
  const id = `${CI_PAGE_TEST_IDS.runRow}.${row.id}`
  const leading: NestedNode = { id: `${id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: row.tone, size: `sm` } }
  if (phone) {
    return { id, component: `Row`, props: { title: row.title, subtitle: row.phoneSubtitle, meta: row.phoneMeta, pressable: true }, slots: { leading }, on: openRun(row.id) }
  }
  const age = relative(row.at)
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
    slots: { leading, trailing: hstack(`${id}.trailing`, trailing) },
    on: openRun(row.id),
  }
}

/** Runs as day Sections (In progress first), or the EmptyState. */
function runSections(id: string, input: CiPageInput, filter: CiRunFilter, kinds: readonly CiRunKind[], phone: boolean, empty: string): NestedNode {
  const groups = ciRunGroups(input, filter, kinds)
  if (groups.length === 0) return { id, component: `EmptyState`, props: { icon: `nav-ci`, title: empty, size: `list` } }
  return vstack(
    id,
    groups.map((group) => ({
      id: `${id}.${group.id}`,
      component: `Section`,
      props: { title: group.title, ...(group.count !== null ? { count: group.count } : {}) },
      children: group.rows.map((row) => runRow(row, phone)),
    }))
  )
}

const filtered = (filter: CiRunFilter) => filter.status !== `all` || filter.repositoryId !== null

// ── Pull requests ─────────────────────────────────────────────────────────

/** One PR (or branch): its rolled-up status, the deciding run's line and,
 *  wide, the merge queue pill; its runs fold under it (compact rows). */
function subjectRow(row: CiSubjectRow, phone: boolean): NestedNode {
  const id = `${CI_PAGE_TEST_IDS.prRow}.${row.id}`
  const trailing: NestedNode | null = phone
    ? null
    : hstack(`${id}.trailing`, [
        { id: `${id}.meta`, component: `Text`, props: { text: row.meta, variant: `muted` } },
        ...(row.pill ? [{ id: `${id}.pill`, component: `Chip`, props: { label: row.pill.text, dot: DOT[row.pill.tone] } }] : []),
      ])
  return {
    id,
    component: `Row`,
    props: {
      ...(row.identifier ? { identifier: row.identifier } : {}),
      title: row.title,
      subtitle: phone ? row.phoneSubtitle : row.subtitle,
      ...(phone ? { meta: row.meta } : {}),
      pressable: true,
      open: false,
    },
    slots: {
      leading: { id: `${id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: row.tone, size: `sm` } },
      ...(trailing ? { trailing } : {}),
    },
    children: row.runs.map((run) => ({
      id: `${id}.run.${run.id}`,
      component: `Row`,
      props: { title: run.workflow, meta: run.phoneMeta, density: `compact`, pressable: true },
      slots: { leading: { id: `${id}.run.${run.id}.glyph`, component: `Icon`, props: { name: run.glyph, tone: run.tone, size: `sm` } } },
      on: openRun(run.id),
    })),
    on: openRun(row.runId),
  }
}

function subjectSections(id: string, groups: CiSubjectGroup[], phone: boolean, empty: string): NestedNode {
  if (groups.length === 0) return { id, component: `EmptyState`, props: { icon: `git-pull-request`, title: empty, size: `list` } }
  return vstack(
    id,
    groups.map((group) => ({
      id: `${id}.${group.id}`,
      component: `Section`,
      props: { title: group.title, count: group.count, ...(group.open ? {} : { collapsible: true, open: false }) },
      children: group.rows.map((row) => subjectRow(row, phone)),
    }))
  )
}

// ── Merge queue ───────────────────────────────────────────────────────────

function nowCard(now: CiQueueNow, id: string): NestedNode {
  const copy = CI_PAGE_COPY.queue
  const children: NestedNode[] = [
    now.progress === null
      ? { id: `${id}.elapsed`, component: `Text`, props: { text: now.elapsed, variant: `muted` } }
      : { id: `${id}.progress`, component: `Progress`, props: { value: now.progress, max: 100, label: now.elapsed, valueText: now.eta, tone: `info` } },
    {
      id: `${id}.members`,
      component: `Section`,
      props: { title: copy.batch, count: now.members.length },
      children: now.members.map((m) => ({
        id: `${id}.member.${m.pullRequestId}`,
        component: `Row`,
        props: { identifier: m.identifier, title: m.title, pressable: true },
        ...(m.author
          ? {
              slots: {
                trailing: {
                  id: `${id}.member.${m.pullRequestId}.author`,
                  component: `Avatar`,
                  props: { name: m.author.name, ...(m.author.image ? { src: m.author.image } : {}), seed: m.author.id, size: `sm` },
                },
              },
            }
          : {}),
        on: openPr(m.pullRequestId),
      })),
    },
  ]
  if (now.runId) {
    children.push({
      id: `${id}.open`,
      component: `Button`,
      props: { label: copy.openRun, icon: `external-link`, variant: `outline`, size: `sm` },
      on: openRun(now.runId),
    })
  }
  return { id, component: `Card`, props: { title: copy.testing, description: now.subtitle }, children: [vstack(`${id}.body`, children)] }
}

function queueRow(row: CiQueueRow, id: string, phone: boolean): NestedNode {
  const copy = CI_PAGE_COPY.queue
  const trailing: NestedNode[] = []
  if (row.at && !phone) trailing.push({ id: `${id}.at`, component: `Text`, props: { text: relative(row.at), variant: `muted` } })
  if (row.badge) trailing.push({ id: `${id}.badge`, component: `Chip`, props: { label: row.badge.text, dot: DOT[row.badge.tone] } })
  if (row.action === `open-log`) {
    const press = row.jobId
      ? { event: { name: CI_PAGE_EVENTS.openJob, context: { run: row.runId, job: row.jobId } } }
      : { event: { name: CI_PAGE_EVENTS.openUrl, context: { url: row.url } } }
    trailing.push({
      id: `${id}.log`,
      component: `Button`,
      props: { label: copy.openLog, icon: `file-text`, variant: `outline`, size: `sm` },
      on: { press },
    })
  }
  if (row.action === `resume`) {
    trailing.push({
      id: `${id}.resume`,
      component: `Button`,
      props: { label: copy.resume, variant: `outline`, size: `sm` },
      on: { press: { event: { name: CI_PAGE_EVENTS.resume, context: { pr: row.pullRequestId } } } },
    })
  }
  if (row.menu) {
    trailing.push({
      id: `${id}.menu`,
      component: `Menu`,
      props: {
        label: copy.more,
        items: [
          { label: copy.menu.mergeNow, value: `merge-now` },
          { label: copy.menu.hold, value: `hold` },
        ],
      },
      children: [{ id: `${id}.menu.button`, component: `Button`, props: { icon: `ellipsis`, variant: `ghost`, size: `icon-xs`, label: copy.more } }],
      on: { select: { event: { name: CI_PAGE_EVENTS.queueMenu, context: { pr: row.pullRequestId } } } },
    })
  }
  return {
    id,
    component: `Row`,
    props: {
      identifier: row.identifier,
      title: row.title,
      ...(row.subtitle ? { subtitle: row.subtitle } : {}),
      ...(row.at && phone ? { meta: relative(row.at) } : {}),
      pressable: true,
    },
    slots: {
      leading: { id: `${id}.glyph`, component: `Icon`, props: { name: row.glyph, tone: row.tone, size: `sm` } },
      ...(trailing.length > 0 ? { trailing: trailing.length === 1 ? trailing[0]! : hstack(`${id}.trailing`, trailing) } : {}),
    },
    on: openPr(row.pullRequestId),
  }
}

function queueSection(id: string, title: string, rows: CiQueueRow[], phone: boolean, fold = false): NestedNode | null {
  if (rows.length === 0) return null
  return {
    id,
    component: `Section`,
    props: { title, count: rows.length, ...(fold ? { collapsible: true, open: rows.length <= 5 } : {}) },
    children: rows.map((row) => queueRow(row, `${CI_PAGE_TEST_IDS.queueRow}.${row.id}`, phone)),
  }
}

function queueRepo(repo: CiQueueRepo, phone: boolean, header: boolean): NestedNode {
  const copy = CI_PAGE_COPY.queue
  const id = `${CI_PAGE_TEST_IDS.queueRepo}.${repo.repositoryId}`
  const children: NestedNode[] = []
  if (header) {
    children.push(
      hstack(`${id}.head`, [
        { id: `${id}.title`, component: `Heading`, props: { text: repo.title, level: `h3` } },
        { id: `${id}.caption`, component: `Text`, props: { text: repo.caption, variant: `muted` } },
      ])
    )
  }
  if (!phone) {
    const stat = (key: keyof typeof copy.stats, value: string) => ({ id: `${id}.stat.${key}`, component: `StatTile`, props: { label: copy.stats[key], value } })
    children.push({
      id: `${CI_PAGE_TEST_IDS.queueStats}.${repo.repositoryId}`,
      component: `Grid`,
      props: { columns: { base: 2, md: 4 }, gap: `sm` },
      children: [
        stat(`inQueue`, String(repo.stats.inQueue)),
        stat(`landedToday`, String(repo.stats.landedToday)),
        stat(`sentBackToday`, String(repo.stats.sentBackToday)),
        stat(`typicalRun`, repo.stats.typicalRun ?? `—`),
      ],
    })
  }
  for (const now of repo.testing) children.push(nowCard(now, `${CI_PAGE_TEST_IDS.queueNow}.${now.trainId}`))
  const upNext = queueSection(`${id}.next`, copy.upNext, repo.upNext, phone)
  const waiting = queueSection(`${id}.waiting`, copy.notReady, repo.waiting, phone, true)
  const sentBack = queueSection(`${id}.sent`, copy.sentBack, repo.sentBack, phone)
  const landed = queueSection(`${id}.landed`, copy.landed, repo.landed, phone)
  const present = (nodes: Array<NestedNode | null>) => nodes.filter((n): n is NestedNode => n !== null)
  if (phone) children.push(...present([upNext, sentBack, waiting, landed]))
  else {
    const left = present([upNext, waiting])
    const right = present([sentBack, landed])
    if (left.length > 0 && right.length > 0) {
      children.push({
        id: `${id}.columns`,
        component: `Grid`,
        props: { columns: { base: 1, lg: 2 }, gap: `lg` },
        children: [vstack(`${id}.left`, left, `lg`), vstack(`${id}.right`, right, `lg`)],
      })
    } else children.push(...left, ...right)
  }
  return vstack(id, children, `lg`)
}

function queuePanel(input: CiPageInput, phone: boolean, repositoryId: string | null): NestedNode {
  const repos = ciQueue(input).filter((repo) => repositoryId === null || repo.repositoryId === repositoryId)
  if (repos.length === 0) {
    return { id: CI_PAGE_TEST_IDS.queue, component: `EmptyState`, props: { icon: `pr-batch`, title: CI_PAGE_COPY.empty.train, size: `list` } }
  }
  return vstack(
    CI_PAGE_TEST_IDS.queue,
    repos.map((repo) => queueRepo(repo, phone, repositoryId === null)),
    `xl`
  )
}

// ── Runners ───────────────────────────────────────────────────────────────

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
          ? { "@call": `fill`, args: { template: copy.offlineSince, params: { time: relative(row.offlineSince) } } }
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

// ── The page ──────────────────────────────────────────────────────────────

/** The CI page's body (ci-page.json LAYOUT): the CURRENT tab's panel. The
 *  tab switcher is the platform's own segmented control (#EXP-2169 "one
 *  look"; labels copy.tabs, phones copy.phoneTabs; counts from
 *  `ciTabCounts`), drawn by the host above this tree. */
export function ciPageTree(input: CiPageInput, state: CiPageState): NestedNode {
  const copy = CI_PAGE_COPY
  switch (state.tab) {
    case `train`:
      return queuePanel(input, state.phone, null)
    case `prs`:
      return vstack(`${CI_PAGE_TEST_IDS.page}.prs`, [
        filters(input, state),
        subjectSections(CI_PAGE_TEST_IDS.prs, ciPrGroups(input, state.filter), state.phone, filtered(state.filter) ? copy.empty.filtered : copy.empty.prs),
      ])
    case `releases`:
      return vstack(`${CI_PAGE_TEST_IDS.page}.releases`, [
        filters(input, state),
        runSections(CI_PAGE_TEST_IDS.releases, input, state.filter, [`release`], state.phone, filtered(state.filter) ? copy.empty.filtered : copy.empty.releases),
      ])
    default:
      return runnersPanel(input)
  }
}

/** The repo page's Runs tab's own switch (#EXP-2214): the CI page's first
 *  three tabs, one repository. */
export type CiRepoTab = Exclude<CiTab, `runners`>

/** THE REPO PAGE'S RUNS TAB (#EXP-2214): a `Segmented` of Merge queue ·
 *  Pull requests · Releases (copy.tabs, phones copy.phoneTabs; `ci-tab`
 *  carries the value), then that panel for ONE repository: no repository
 *  header, no filter bar. */
export function ciRepoRunsTree(input: CiPageInput, repositoryId: string, tab: CiRepoTab, phone: boolean): NestedNode {
  const copy = CI_PAGE_COPY
  const labels = phone ? copy.phoneTabs : copy.tabs
  const filter: CiRunFilter = { status: `all`, repositoryId, source: null }
  const panel =
    tab === `train`
      ? queuePanel(input, phone, repositoryId)
      : tab === `prs`
        ? subjectSections(CI_PAGE_TEST_IDS.prs, ciPrGroups(input, filter), phone, copy.empty.prs)
        : runSections(CI_PAGE_TEST_IDS.releases, input, filter, [`release`], phone, copy.empty.releases)
  return vstack(CI_PAGE_TEST_IDS.runs, [
    {
      id: CI_PAGE_TEST_IDS.tabs,
      component: `Segmented`,
      props: { value: tab, size: `sm`, items: ([`train`, `prs`, `releases`] as const).map((value) => ({ label: labels[value], value })) },
      on: { change: { event: { name: CI_PAGE_EVENTS.tab } } },
    },
    panel,
  ])
}
