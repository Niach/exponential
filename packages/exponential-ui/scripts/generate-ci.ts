// #EXP-2204 (docs/ci-components-contract.md): the CI half of the generator
// (scripts/generate.ts merges it): the demo data of the LogView, JobGraph
// and CodeBlock specimens, and the two rule fixtures every port replays:
//   fixtures/ci-log.json     anchors, logModel, logOpenDefault, logRows,
//                            logSearch, logReveal, blameBlocks, the status table
//   fixtures/job-graph.json  jobGraphLayout (graph + compact) and jobGraphUpstream
// Demo data only (no real repository, person or run).

import {
  CI_STATUSES,
  CI_STATUS_SPIN_MS,
  blameBlocks,
  ciStatusEntry,
  extendLineAnchor,
  formatLineAnchor,
  logModel,
  logOpenDefault,
  logReveal,
  logRows,
  logSearch,
  parseLineAnchor,
} from "../src/ci-log"
import { JOB_GRAPH, jobGraphLayout, jobGraphUpstream } from "../src/job-graph"
import type { NestedNode } from "../src/types"

const E = `\u001b`
const green = (t: string) => `${E}[32m${t}${E}[0m`
const red = (t: string) => `${E}[31m${t}${E}[0m`
const dim = (t: string) => `${E}[2m${t}${E}[0m`

/** A line printed at 09:14:<s> (+ the runner's 7-digit fraction). */
const at = (s: number, text: string) => `2026-11-27T09:${String(14 + Math.floor(s / 60)).padStart(2, `0`)}:${String(s % 60).padStart(2, `0`)}.${String(1000000 + s * 7919).slice(-7)}Z ${text}`

/** A finished job whose Unit tests step failed (the specimen and the fixture). */
const FAILED_JOB_RAW: string[] = [
  `##[group]Set up job`,
  `Current runner version: '2.329.0'`,
  `Runner name: 'runner-3'`,
  `Operating system: Ubuntu 26.04 LTS`,
  `##[endgroup]`,
  `##[group]Checkout`,
  `Syncing repository: acme/shop`,
  `HEAD is now at 4f2c9e1 Cache the cart totals`,
  `##[endgroup]`,
  `##[group]Install`,
  `$ bun install --frozen-lockfile`,
  dim(`Resolved 1204 packages in 812ms`),
  `${green(`+`)} 1204 packages installed [2.41s]`,
  `##[endgroup]`,
  `##[group]Unit tests`,
  `$ bun test`,
  `src/cart.test.ts:`,
  `${green(`(pass)`)} cart > adds an item [0.4ms]`,
  `${green(`(pass)`)} cart > sums the totals [0.2ms]`,
  `src/store.test.ts:`,
  `${green(`(pass)`)} store > put [0.2ms]`,
  `${red(`(fail)`)} store > get returns the cached row [1.2ms]`,
  `  expect(received).toEqual(expected)`,
  `  ${green(`- Expected  - 1`)}`,
  `  ${red(`+ Received  + 1`)}`,
  `    Object {`,
  `  ${green(`-   "qty": 2,`)}`,
  `  ${red(`+   "qty": 1,`)}`,
  `    }`,
  `      at src/store.test.ts:42:18`,
  `${green(`(pass)`)} store > evicts after 60s [0.3ms]`,
  ``,
  ` 4 pass`,
  ` 1 fail`,
  `Ran 5 tests across 2 files. [182ms]`,
  `##[error]Process completed with exit code 1.`,
  `##[endgroup]`,
  `##[group]Post Checkout`,
  `Cleaning up orphan processes`,
  `##[endgroup]`,
]
const FAILED_JOB = FAILED_JOB_RAW.map((l, i) => at(i * 2 + 1, l))
const FAILED_SECTIONS = [
  { status: `passed`, duration: `2s` },
  { status: `passed`, duration: `1s` },
  { status: `passed`, duration: `3s` },
  { status: `failed`, duration: `41s` },
  { status: `passed`, duration: `0s` },
]

/** A job still running its Build step (no endgroup yet: running by default). */
const RUNNING_JOB: string[] = [
  `##[group]Set up job`,
  `Runner name: 'runner-1'`,
  `##[endgroup]`,
  `##[group]Install`,
  `$ bun install --frozen-lockfile`,
  dim(`Resolved 1204 packages in 790ms`),
  `##[endgroup]`,
  `##[group]Build`,
  `$ bun run build`,
  `vite v7.2.1 building for production...`,
  `${green(`✓`)} 1873 modules transformed.`,
  `rendering chunks (12)...`,
  `dist/assets/index-4f2c9e1.js   412.80 kB │ gzip: 131.02 kB`,
  `dist/assets/cart-9a1b7d3.js     38.12 kB │ gzip:  11.40 kB`,
  `computing gzip size (14)...`,
]

// ---------------------------------------------------------------------------
// Specimens
// ---------------------------------------------------------------------------

const logView = (id: string, props: Record<string, unknown>, annotations?: NestedNode): NestedNode => ({
  id,
  component: `LogView`,
  props,
  ...(annotations ? { slots: { annotations } } : {}),
  on: { select: { event: { name: `anchor` } }, toggle: { event: { name: `fold` } }, search: { event: { name: `find` } } },
})

/** The hand-written specimens (scripts/generate.ts SPECIMEN_OVERRIDES). */
export const CI_SPECIMENS: Record<string, { label: string; node: NestedNode }[]> = {
  LogView: [
    {
      label: `a failed job · timestamps · search`,
      node: logView(
        `job_log`,
        { lines: FAILED_JOB, sections: FAILED_SECTIONS, open: [3], query: `store`, timestamps: true, follow: false, maxLines: 18 },
        { id: `job_log_failure`, component: `Alert`, props: { type: `error`, title: `1 test failed`, message: `store > get returns the cached row (src/store.test.ts:42)` } }
      ),
    },
    {
      label: `running · follow`,
      node: logView(`live_log`, { lines: RUNNING_JOB, sections: [{ status: `passed`, duration: `1s` }, { status: `passed`, duration: `4s` }], follow: true, maxLines: 8 }),
    },
  ],
  JobGraph: [
    {
      label: `stages · a matrix · test selected`,
      node: { id: `pipeline`, component: `JobGraph`, props: { jobs: stagedJobs(), value: `test` }, on: { select: { event: { name: `open_job` } } } },
    },
    {
      label: `needs only`,
      node: { id: `needs_graph`, component: `JobGraph`, props: { jobs: needsJobs() }, on: { select: { event: { name: `open_job` } } } },
    },
  ],
}

function stagedJobs(): Record<string, unknown>[] {
  return [
    { id: `lint`, name: `lint`, stage: `build`, status: `passed`, duration: `38s` },
    { id: `build`, name: `build`, stage: `build`, status: `passed`, duration: `1m 12s` },
    { id: `test_1`, name: `test (linux, 22)`, stage: `test`, matrix: `test`, status: `passed`, needs: [`build`] },
    { id: `test_2`, name: `test (linux, 24)`, stage: `test`, matrix: `test`, status: `passed`, needs: [`build`] },
    { id: `test_3`, name: `test (macos, 22)`, stage: `test`, matrix: `test`, status: `failed`, needs: [`build`] },
    { id: `test_4`, name: `test (macos, 24)`, stage: `test`, matrix: `test`, status: `passed`, needs: [`build`] },
    { id: `test_5`, name: `test (windows, 24)`, stage: `test`, matrix: `test`, status: `running`, needs: [`build`] },
    { id: `e2e`, name: `e2e`, stage: `test`, status: `running`, duration: `2m 4s`, needs: [`build`] },
    { id: `deploy`, name: `deploy`, stage: `deploy`, status: `queued`, needs: [`test`, `e2e`] },
    { id: `docs`, name: `docs`, stage: `deploy`, status: `skipped`, needs: [`lint`] },
  ]
}

function needsJobs(): Record<string, unknown>[] {
  return [
    { id: `checkout`, name: `checkout`, status: `passed`, duration: `4s` },
    { id: `build`, name: `build`, status: `passed`, duration: `1m 2s`, needs: [`checkout`] },
    { id: `lint`, name: `lint`, status: `passed`, duration: `21s`, needs: [`checkout`] },
    { id: `test`, name: `test`, status: `failed`, duration: `48s`, needs: [`build`] },
    { id: `package`, name: `package`, status: `passed`, duration: `33s`, needs: [`build`] },
    { id: `publish`, name: `publish`, status: `cancelled`, needs: [`test`, `package`, `lint`] },
  ]
}

/** The code browser's file (a demo merge train). */
const TRAIN_TS = [
  `import { runChecks } from "./checks"`,
  `import type { Pull } from "./types"`,
  `/** The merge train: one batch of green pulls, bisected on a red run. */`,
  `export interface Batch {`,
  `  pulls: Pull[]`,
  `  base: string`,
  `}`,
  ``,
  `export function compose(pulls: Pull[], max = 4): Batch[] {`,
  `  const batches: Batch[] = []`,
  `  for (let i = 0; i < pulls.length; i += max) batches.push({ pulls: pulls.slice(i, i + max), base: "main" })`,
  `  return batches`,
  `}`,
  ``,
  `export async function bisect(batch: Batch): Promise<Pull[]> {`,
  `  if (batch.pulls.length <= 1) return batch.pulls`,
  `  const half = Math.ceil(batch.pulls.length / 2)`,
  `  const left = { ...batch, pulls: batch.pulls.slice(0, half) }`,
  `  const green = await runChecks(left)`,
  `  return green ? bisect({ ...batch, pulls: batch.pulls.slice(half) }) : bisect(left)`,
  `}`,
].join(`\n`)
const TRAIN_BLAME = [
  { start: 1, author: `Ada Lovelace`, age: `3 weeks ago` },
  { start: 9, author: `Grace Hopper`, age: `2 days ago` },
  { start: 15, author: `Ada Lovelace`, age: `5 hours ago` },
]

/** The CodeBlock specimen's new cases (appended to SPECIMEN_EXTRA.CodeBlock). */
export const CI_CODE_BLOCK_EXTRAS: { label: string; node: NestedNode }[] = [
  {
    label: `anchors · selection L9-L11`,
    node: { id: `code_block_anchors`, component: `CodeBlock`, props: { title: `src/train.ts`, language: `ts`, code: TRAIN_TS, anchors: true, selection: `L9-L11` }, on: { select: { event: { name: `anchor` } } } },
  },
  {
    label: `gutter · blame`,
    node: { id: `code_block_blame`, component: `CodeBlock`, props: { title: `src/train.ts`, language: `ts`, code: TRAIN_TS, lineNumbers: true, gutter: `blame`, blame: TRAIN_BLAME } },
  },
]

// ---------------------------------------------------------------------------
// fixtures/ci-log.json
// ---------------------------------------------------------------------------

const ANCHOR_TEXTS: unknown[] = [
  `L12`, `#L12`, `L6-L9`, `#L9-L6`, `L7-L7`, `L1`, `L007`, `L999999999`, `L1000000000`,
  `L0`, `L-1`, `l12`, `L12-`, `L12-9`, `L12-L`, `L12-L0`, `L 12`, ` L12`, `L12 `, `##L12`, `#`, ``, `L+3`, `L1.5`, `L١٢`, 12, null,
]
const FORMATS: [number, number?][] = [[12], [12, 12], [6, 9], [9, 6], [1, 1000]]
const EXTENDS: [unknown, number, boolean][] = [
  [null, 5, false], [null, 5, true], [`L3`, 9, false], [`L3`, 9, true], [`L9`, 3, true], [`L6-L9`, 12, true], [`L6-L9`, 2, true], [`L6-L9`, 6, true], [`nope`, 4, true], [`#L4`, 4, true],
]

interface LogCase {
  name: string
  lines: unknown[]
  sections?: unknown[]
  /** Open sets logRows / logReveal are replayed with (+ the default). */
  open?: unknown[][]
  queries?: unknown[]
  reveal?: number[]
}

const LOG_CASES: LogCase[] = [
  {
    name: `a failed job: timestamps, ANSI, ##[error], meta statuses`,
    lines: FAILED_JOB,
    sections: FAILED_SECTIONS,
    open: [[], [0, 3], [4, 4, 1, `x`, 2.5, -1, 9]],
    queries: [`store`, `STORE`, `pass`, `(fail)`, `qty": `, `zzz`, ``, null],
    reveal: [1, 8, 22, 40, 99],
  },
  {
    name: `a running job: the unclosed last group runs`,
    lines: RUNNING_JOB,
    sections: [{ status: `passed`, duration: `1s` }, { status: `passed`, duration: `4s` }],
    open: [[2]],
    queries: [`kB`, `gzip`],
    reveal: [3, 9],
  },
  {
    name: `lines outside sections, an empty section, a nested group closes the open one`,
    lines: [
      `before any step`,
      `##[group]Empty`,
      `##[endgroup]`,
      `between`,
      `##[group]  Outer  `,
      `outer line`,
      `##[group]Inner`,
      `inner line`,
      `##[endgroup]`,
      `after inner`,
      `##[endgroup]`,
      `##[group]`,
      `##[endgroup]`,
      `the end`,
    ],
    open: [[], [0, 1, 2, 3]],
    queries: [`line`, `e`],
    reveal: [1, 3, 4, 6],
  },
  {
    name: `prefixes: error, warning, notice, debug, ANSI before a marker, prefix in a group title`,
    lines: [
      `##[group]Lint`,
      `##[warning]Unused variable 'x'`,
      `##[notice]Cache hit`,
      `##[debug]exit 0`,
      `${E}[31m##[error]${E}[0mType 'string' is not assignable`,
      `${E}[33m##[group]${E}[0mColoured title`,
      `  ##[error] indented: no prefix`,
      `##[ERROR]upper case: no prefix`,
      `##[endgroup] trailing text ignored`,
      `##[group]##[error]a title`,
      `##[errors] not a prefix`,
    ],
    sections: [null, { status: `bogus` }, { duration: `3s` }],
    queries: [`prefix`, `##[`],
  },
  {
    name: `timestamps: zones, fractions, malformed`,
    lines: [
      `2026-11-27T09:14:05Z plain zulu`,
      `2026-11-27T09:14:05.5+02:00 offset and fraction`,
      `2026-11-27T23:59:60-05:30 leap second as written`,
      `2026-11-27T09:14:05Z`,
      `2026-11-27T09:14:05Z  two spaces: one is text`,
      `2026-11-27 09:14:05Z not ISO`,
      `2026-11-27T9:14:05Z single-digit hour`,
      `2026-11-27T09:14:05 no zone`,
      `2026-11-27T09:14:05Z ##[group]Stamped step`,
      `2026-11-27T09:14:06Z inside`,
    ],
    queries: [`z`, ` `],
  },
  {
    name: `search: case folding is ASCII only, offsets are code points, matches never overlap`,
    lines: [`aaaa`, `AaA`, `ÄÖÜ äöü ÄÖÜ`, `Straße STRASSE`, `😀 emoji then ok OK`, `${E}[1mbo${E}[0mld bold`, `ǅ ǆ`, `İi`],
    queries: [`aa`, `A`, `äö`, `ÄÖ`, `strasse`, `ss`, `ok`, `😀`, `bold`, `ǆ`, `i`],
  },
  {
    name: `no lines`,
    lines: [],
    queries: [`x`],
    reveal: [1],
  },
]

const BLAME_CASES: { name: string; blame: unknown[]; lineCount: number }[] = [
  { name: `the code browser's three blocks`, blame: TRAIN_BLAME, lineCount: 21 },
  { name: `unsorted, lines before the first entry`, blame: [{ start: 6, author: `B` }, { start: 3, author: `A` }], lineCount: 8 },
  { name: `a tie: the later entry wins, the earlier is empty`, blame: [{ start: 2, author: `A` }, { start: 2, author: `B` }, { start: 4, author: `C` }], lineCount: 5 },
  { name: `invalid starts are ignored`, blame: [{ start: 0 }, { start: 1.5 }, { start: `3` }, null, `x`, { author: `no start` }, { start: 2, author: `ok` }], lineCount: 4 },
  { name: `an entry past the end`, blame: [{ start: 1, author: `A` }, { start: 9, author: `B` }], lineCount: 3 },
  { name: `no lines`, blame: [{ start: 1, author: `A` }], lineCount: 0 },
  { name: `no blame`, blame: [], lineCount: 2 },
]

export function ciLogFixture(header: string) {
  return {
    $comment: `${header} #EXP-2204 (docs/ci-components-contract.md §1-§3, §6): the LogView/LogLines and CodeBlock rules (src/ci-log.ts; eui-core ci_log.rs, Swift CiLog.swift, Kotlin CiLog.kt replay this file byte for byte). \`statusTable\`: per ciStatus (+ an unknown and a missing one, read as queued) its glyph, spin, mark colour, text colour and a11y string (style.json ciStatus + the neutral theme). \`anchors\`: parseLineAnchor(text) (null = no anchor), formatLineAnchor(start, end?), extendLineAnchor(current, line, shift). \`logs\`: per case logModel(lines, sections) → \`model\`, logOpenDefault → \`openDefault\`, logRows(model, open) for the default and every listed open set, logSearch(model, query) per query (code-point offsets into the escape-free text), logReveal(model, openDefault, line) per line. \`blame\`: blameBlocks(blame, lineCount): per line {block (index into blame, -1 = none), first, border}.`,
    statusTable: {
      spinMs: CI_STATUS_SPIN_MS,
      rows: [...CI_STATUSES, `bogus`, null].map((status) => ({ input: status, ...ciStatusEntry(status) })),
    },
    anchors: {
      parse: ANCHOR_TEXTS.map((text) => ({ text, range: parseLineAnchor(text) })),
      format: FORMATS.map(([start, end]) => ({ start, ...(end !== undefined ? { end } : {}), anchor: formatLineAnchor(start, end) })),
      extend: EXTENDS.map(([current, line, shift]) => ({ current, line, shift, anchor: extendLineAnchor(current, line, shift) })),
    },
    logs: LOG_CASES.map((c) => {
      const model = logModel(c.lines, c.sections)
      const openDefault = logOpenDefault(model)
      return {
        name: c.name,
        lines: c.lines,
        ...(c.sections ? { sections: c.sections } : {}),
        model,
        openDefault,
        rows: [{ open: openDefault, rows: logRows(model, openDefault) }, ...(c.open ?? []).map((open) => ({ open, rows: logRows(model, open) }))],
        search: (c.queries ?? []).map((query) => ({ query, matches: logSearch(model, query) })),
        reveal: (c.reveal ?? []).map((line) => ({ open: openDefault, line, result: logReveal(model, openDefault, line) })),
      }
    }),
    blame: BLAME_CASES.map((c) => ({ ...c, lines: blameBlocks(c.blame, c.lineCount) })),
  }
}

// ---------------------------------------------------------------------------
// fixtures/job-graph.json
// ---------------------------------------------------------------------------

const GRAPH_CASES: { name: string; jobs: unknown[]; upstream?: string[] }[] = [
  { name: `stages build → test (a 3/5 matrix) → deploy`, jobs: stagedJobs(), upstream: [`deploy`, `test`, `docs`, `lint`, `test_1`, `nope`] },
  { name: `needs only: columns by needs depth`, jobs: needsJobs(), upstream: [`publish`, `build`, `checkout`] },
  {
    name: `a cycle breaks on the current path; unknown needs are dropped`,
    jobs: [
      { id: `a`, needs: [`c`] },
      { id: `b`, needs: [`a`, `ghost`] },
      { id: `c`, needs: [`b`] },
      { id: `d`, needs: [`d`, `a`, `a`] },
      { id: `e`, needs: [`ghost`] },
    ],
    upstream: [`a`, `d`, `e`],
  },
  {
    name: `a stage-less job takes the "" column; stages in first-appearance order`,
    jobs: [
      { id: `deploy`, stage: `deploy`, needs: [`build`] },
      { id: `notify` },
      { id: `build`, stage: `build`, status: `running` },
      { id: `smoke`, stage: `deploy`, needs: [`deploy`] },
    ],
    upstream: [`smoke`],
  },
  {
    name: `matrix: aggregate status order, needs through the collapse, the key as a need`,
    jobs: [
      { id: `setup`, status: `passed` },
      { id: `m1`, matrix: `unit`, status: `skipped`, needs: [`setup`] },
      { id: `m2`, matrix: `unit`, status: `cancelled`, needs: [`setup`, `m1`] },
      { id: `m3`, matrix: `unit`, status: `passed`, needs: [`lint`] },
      { id: `lint`, status: `queued` },
      { id: `solo`, matrix: `one`, status: `passed`, duration: `9s` },
      { id: `report`, needs: [`m2`, `unit`, `one`] },
    ],
    upstream: [`report`],
  },
  {
    name: `normalising: no id, repeated ids, a name, unknown statuses, a key that names a job`,
    jobs: [
      { name: `no id` },
      `x`,
      null,
      { id: `one`, name: `First`, status: `weird`, duration: 12 },
      { id: `one`, name: `Repeat` },
      { id: `k1`, matrix: `one`, status: `passed` },
      { id: `two`, matrix: ``, needs: [`one`, 3] },
      { id: `k2`, matrix: `two` },
    ],
  },
  { name: `no jobs`, jobs: [] },
]

export function jobGraphFixture(header: string) {
  return {
    $comment: `${header} #EXP-2204 (docs/ci-components-contract.md §5): the JobGraph layout (src/job-graph.ts; eui-core job_graph.rs, Swift JobGraphLayout.swift, Kotlin JobGraphLayout.kt replay this file byte for byte). \`geometry\` = style.json jobGraph. Per case: jobGraphLayout(jobs, false) → \`graph\` (stage columns or needs depth, matrix collapse, node boxes, need edges as cubics), jobGraphLayout(jobs, true) → \`list\` (the compact rows below md) and jobGraphUpstream(graph, id) per listed id (the node + its ancestors, in node order).`,
    geometry: JOB_GRAPH,
    cases: GRAPH_CASES.map((c) => {
      const graph = jobGraphLayout(c.jobs, false)
      return {
        name: c.name,
        jobs: c.jobs,
        graph,
        list: jobGraphLayout(c.jobs, true),
        upstream: (c.upstream ?? []).map((id) => ({ id, ids: jobGraphUpstream(graph, id) })),
      }
    }),
  }
}
