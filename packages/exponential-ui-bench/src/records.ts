// VAPP-202 (eApps §15): the `records-25k` scenario's half that every runner
// ports: the rows (`recordRows`, the Rust core's `record_rows` in
// examples/bench_suite.rs builds the same ones) and the measurement
// (`runRecordsQueries`): each query parsed once, run `warmup` times, then
// `runs` times timed; p50 / p95 = the sorted timings at floor(runs × 0.5 /
// 0.95). Pure, so the web runners bundle it into the page and time the
// TS twin in Chromium.

import { parseRecordsJson, parseRecordsQuery, recordsValue, type RecordRow, type RecordsQuery } from "@exponential_app/eui/host/query"

/** The rows' generator spec (scenarios/records-25k.json `data.records`). */
export interface RecordsSpec {
  count: number
  kind: string
  authors: string[]
  /** `YYYY-MM-DD` of row 0; row i is `i % days` days later. */
  start: string
  days: number
  projects: number
  /** `hours` = (i % hoursModulo) + 0.5. */
  hoursModulo: number
  /** Every row whose i % noteEvery ≠ 0 carries a `note`. */
  noteEvery: number
  createdAt: string
}

/** One query of the scenario: the URL sugar's params, or the JSON form. */
export interface RecordsBenchQuery {
  id: string
  title: string
  params?: Record<string, string>
  json?: unknown
}

export interface RecordsSteps {
  me: string
  now: string
  warmup: number
  runs: number
  queries: RecordsBenchQuery[]
}

const DAYS_IN_MONTH = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
const leap = (y: number) => (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0

/** `start` + n days as `YYYY-MM-DD`. */
export function addDays(start: string, n: number): string {
  let [y, m, d] = start.split(`-`).map(Number) as [number, number, number]
  d += n
  const len = (m: number) => (m === 2 && leap(y) ? 29 : DAYS_IN_MONTH[m - 1]!)
  while (d > len(m)) {
    d -= len(m)
    m += 1
    if (m > 12) {
      m = 1
      y += 1
    }
  }
  return `${String(y).padStart(4, `0`)}-${String(m).padStart(2, `0`)}-${String(d).padStart(2, `0`)}`
}

export function recordRows(spec: RecordsSpec): RecordRow[] {
  return Array.from({ length: spec.count }, (_, i) => {
    const data: Record<string, unknown> = { date: addDays(spec.start, i % spec.days), hours: (i % spec.hoursModulo) + 0.5, project: `p${i % spec.projects}` }
    if (i % spec.noteEvery !== 0) data.note = `n${i}`
    const author = spec.authors[i % spec.authors.length]!
    return { id: `r${i}`, kind: spec.kind, data, createdBy: author, authorId: author, createdAt: spec.createdAt, updatedAt: spec.createdAt }
  })
}

export function parseBenchQuery(q: RecordsBenchQuery, kind: string): RecordsQuery {
  return q.json !== undefined ? parseRecordsJson(q.json, kind).query : parseRecordsQuery(q.params ?? {}, kind)
}

/** Per query `<id>P50Ms` / `<id>P95Ms`, and `matched` = Σ count (the same on every runner). */
export function runRecordsQueries(rows: RecordRow[], kind: string, steps: RecordsSteps, now: () => number): Record<string, number> {
  const out: Record<string, number> = {}
  let matched = 0
  for (const q of steps.queries) {
    const query = parseBenchQuery(q, kind)
    const run = () => recordsValue(rows, query, { me: steps.me, now: steps.now, names: {} })
    for (let i = 0; i < steps.warmup; i++) run()
    const ts: number[] = []
    let count = 0
    for (let i = 0; i < steps.runs; i++) {
      const t = now()
      const v = run()
      ts.push(now() - t)
      count = v?.count ?? 0
    }
    ts.sort((a, b) => a - b)
    out[`${q.id}P50Ms`] = ts[Math.floor(steps.runs * 0.5)]!
    out[`${q.id}P95Ms`] = ts[Math.floor(steps.runs * 0.95)]!
    matched += count
  }
  out.matched = matched
  return out
}
