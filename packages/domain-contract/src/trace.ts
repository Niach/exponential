// EXP-1324: the Trace patterns, ONE model ×4. The transcript's tool rows and
// groups, the reasoning line, the subagent line and the failed-tests line are
// all `Trace` lines (shadcn's Trace, ported to Radix in @exp/ui; gpui, ExpUI
// and Compose rebuild it from `fixtures/trace.json` geometry). This file is
// the TS reference; desktop `steer::trace`, iOS ExpCore `Trace.swift` and
// Android `domain/Trace.kt` mirror it and replay the fixture's cases.

import fixture from "../fixtures/trace.json" with { type: "json" }
import { workDurationLong } from "./work-card"

export type TraceStatus = `running` | `error` | `pending`

/** One painted Trace line: a concept icon, the title, the optional muted
 *  description (tabular figures) and the status that tints/shimmers it. */
export interface TraceLineModel {
  icon: string
  title: string
  description: string | null
  status: TraceStatus | null
}

export const TRACE_COPY = fixture.copy
export const TRACE_ICONS = fixture.icons
export const TRACE_GEOMETRY = fixture.geometry

function fill(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (_, key: string) => values[key] ?? ``)
}

function clean(value: string | null | undefined): string {
  return (value ?? ``).replace(/\s+/g, ` `).trim()
}

// ── Tool line ────────────────────────────────────────────────────────────────

export interface TraceToolInput {
  /** Contract `toolKind`; unknown = `other`. */
  toolKind: string
  /** The tool's own name (`Bash`, `mcp__x__y`). */
  name: string
  detail?: string | null
  /** False while the call runs. */
  settled: boolean
  failed: boolean
}

type VerbPair = { running: string; settled: string; icon: string; blank: string }

const VERBS = fixture.tool.verbs as Record<string, VerbPair>

/** A tool call as its Trace line. Verb kinds read `{verb} {detail}`
 *  ("Ran bun test", "Reading README.md"); a blank detail takes the kind's
 *  `blank` object ("Ran a command"). `other`/unknown kinds keep the tool's
 *  own name as the title and the detail as the description. */
export function traceTool(input: TraceToolInput): TraceLineModel {
  const status: TraceStatus | null = input.failed
    ? `error`
    : input.settled
      ? null
      : `running`
  const verbs = VERBS[input.toolKind]
  const detail = clean(input.detail)
  if (!verbs) {
    return {
      icon: TRACE_ICONS.tool,
      title: clean(input.name) || fixture.tool.unnamed,
      description: detail || null,
      status,
    }
  }
  const verb = input.settled || input.failed ? verbs.settled : verbs.running
  return {
    icon: verbs.icon,
    title: `${verb} ${detail || verbs.blank}`,
    description: null,
    status,
  }
}

// ── Reasoning line ───────────────────────────────────────────────────────────

/** "Thinking" (shimmering) while the thought streams; "Thought for 5s" once
 *  it closed; "Thought for a moment" under one second or without a duration. */
export function traceReasoning(input: {
  running: boolean
  durationMs?: number | null
}): TraceLineModel {
  if (input.running) {
    return {
      icon: TRACE_ICONS.reasoning,
      title: fixture.reasoning.running,
      description: null,
      status: `running`,
    }
  }
  const ms = input.durationMs ?? null
  const title =
    ms === null || ms < 1000
      ? fixture.reasoning.brief
      : fill(fixture.reasoning.settled, { duration: workDurationLong(ms) })
  return { icon: TRACE_ICONS.reasoning, title, description: null, status: null }
}

// ── Subagent line ────────────────────────────────────────────────────────────

/** "Running the Explore agent" → "Ran the Explore agent" + a duration BADGE
 *  (the Badge primitive, outline) → "The Explore agent failed". The badge is
 *  `description` here; clients paint it as the badge, not as muted text. */
export function traceSubagent(input: {
  label: string
  running: boolean
  failed: boolean
  durationMs?: number | null
}): TraceLineModel {
  const label = clean(input.label)
  const copy = label ? fixture.subagent.named : fixture.subagent.unnamed
  const template = input.failed ? copy.failed : input.running ? copy.running : copy.settled
  const ms = input.durationMs ?? null
  return {
    icon: TRACE_ICONS.subagent,
    title: fill(template, { label }),
    description: ms === null || ms < 0 ? null : workDurationLong(ms),
    status: input.failed ? `error` : input.running ? `running` : null,
  }
}

// ── Failed tests line ────────────────────────────────────────────────────────

export interface TestResults {
  passed: number
  failed: number
  total: number
  /** The ONE suite every failure names; null when none or several. */
  suite: string | null
}

const INT = (value: string | undefined) => (value ? Number.parseInt(value, 10) : 0)

// eslint-disable-next-line no-control-regex
const ANSI_CSI = /\u001b\[[0-?]*[ -/]*[@-~]/g

/** Parses a command's output as a test run's summary. Recognised (first
 *  match wins, see `fixtures/trace.json` `tests.$comment`): jest, vitest,
 *  bun test, cargo test, pytest, gradle, xcodebuild. Null = not test output.
 *  The suite comes from the failure lines of the same runners. */
export function parseTestResults(output: string | null | undefined): TestResults | null {
  if (!output) return null
  const text = output.replace(ANSI_CSI, ``).replace(/\r\n?/g, `\n`)
  let counts: { passed: number; failed: number; total: number } | null = null

  // jest: `Tests:       1 failed, 11 passed, 12 total`
  let m = /^Tests:\s+(?:(\d+) failed, )?(?:\d+ skipped, )?(?:\d+ todo, )?(?:(\d+) passed, )?(\d+) total/m.exec(text)
  if (m) counts = { failed: INT(m[1]), passed: INT(m[2]), total: INT(m[3]) }
  // vitest: `Tests  1 failed | 11 passed (12)`
  if (!counts) {
    m = /^\s*Tests\s+(?:(\d+) failed)?(?:\s*\|\s*)?(?:(\d+) passed)?[^(\n]*\((\d+)\)/m.exec(text)
    if (m && (m[1] || m[2])) counts = { failed: INT(m[1]), passed: INT(m[2]), total: INT(m[3]) }
  }
  // cargo: `test result: FAILED. 11 passed; 1 failed; 0 ignored` (summed)
  if (!counts) {
    const re = /^test result: \w+\. (\d+) passed; (\d+) failed;/gm
    let passed = 0
    let failed = 0
    let seen = false
    for (const hit of text.matchAll(re)) {
      seen = true
      passed += INT(hit[1])
      failed += INT(hit[2])
    }
    if (seen) counts = { passed, failed, total: passed + failed }
  }
  // xcodebuild: `Executed 12 tests, with 1 failure` (the LAST one = the total)
  if (!counts) {
    const hits = [...text.matchAll(/^\s*Executed (\d+) tests?, with (\d+) failures?/gm)]
    const last = hits[hits.length - 1]
    if (last) {
      const total = INT(last[1])
      const failed = INT(last[2])
      counts = { total, failed, passed: total - failed }
    }
  }
  // gradle: `12 tests completed, 1 failed`
  if (!counts) {
    m = /^(\d+) tests? completed, (\d+) failed/m.exec(text)
    if (m) counts = { total: INT(m[1]), failed: INT(m[2]), passed: INT(m[1]) - INT(m[2]) }
  }
  // pytest: `===== 1 failed, 11 passed in 0.12s =====`
  if (!counts) {
    m = /^=+ (?:(\d+) failed)?(?:, )?(?:(\d+) passed)?[^=\n]* in [\d.]+s(?: \([^)]*\))? =+$/m.exec(text)
    if (m && (m[1] || m[2])) {
      counts = { failed: INT(m[1]), passed: INT(m[2]), total: INT(m[1]) + INT(m[2]) }
    }
  }
  // bun test: ` 11 pass` / ` 1 fail` lines
  if (!counts) {
    const pass = /^\s*(\d+) pass$/m.exec(text)
    const fail = /^\s*(\d+) fail$/m.exec(text)
    if (pass || fail) {
      const passed = INT(pass?.[1])
      const failed = INT(fail?.[1])
      counts = { passed, failed, total: passed + failed }
    }
  }
  if (!counts) return null
  return {
    passed: counts.passed,
    failed: counts.failed,
    total: counts.total,
    suite: failedSuite(text),
  }
}

/** `src/bench/StartupBenchmark.test.ts` → `StartupBenchmark`. */
function fileStem(path: string): string {
  return (path.split(`/`).pop() ?? ``).split(`.`)[0] ?? ``
}

function failedSuite(text: string): string | null {
  const suites = new Set<string>()
  const add = (value: string | undefined) => {
    const name = clean(value)
    if (name) suites.add(name)
  }
  for (const line of text.split(`\n`)) {
    let m: RegExpExecArray | null
    // bun: `(fail) Suite > test [1.2ms]`
    if ((m = /^\(fail\) (.+?) > /.exec(line))) add(m[1])
    // jest/vitest: `FAIL src/a/startup.test.ts` / ` FAIL  src/x.test.ts > Suite > t`
    else if ((m = /^\s*FAIL\s+(\S+?)(?:\s+>\s+(.+?)\s+>)?(?:\s|$)/.exec(line)))
      add(m[2] ?? fileStem(m[1]))
    // gradle: `StartupBenchmark > coldStart FAILED`
    else if ((m = /^(\S+) > .* FAILED$/.exec(line))) add(m[1].split(`.`).pop())
    // pytest: `FAILED tests/test_startup.py::test_cold - assert …`
    else if ((m = /^FAILED (\S+?)::/.exec(line))) add(fileStem(m[1]))
    // xcodebuild: `Test Case '-[AppTests.StartupBenchmark testCold]' failed`
    else if ((m = /^Test Case '-\[(?:\w+\.)?(\w+) \w+\]' failed/.exec(line))) add(m[1])
    // cargo: `---- startup::cold_start stdout ----`
    else if ((m = /^---- (?:(\S+)::)?\S+ stdout ----$/.exec(line))) add(m[1])
  }
  return suites.size === 1 ? [...suites][0] : null
}

/** The failed-tests line for a SETTLED command whose output parsed with at
 *  least one failure; null otherwise (the command stays its "Ran …" line). */
export function traceFailedTests(results: TestResults | null): TraceLineModel | null {
  if (!results || results.failed <= 0) return null
  const tests = results.total === 1 ? fixture.tests.test : fixture.tests.tests
  const title = fill(results.suite ? fixture.tests.titleIn : fixture.tests.title, {
    failed: String(results.failed),
    total: String(results.total),
    tests,
    suite: results.suite ?? ``,
  })
  return { icon: TRACE_ICONS.failed, title, description: null, status: `error` }
}
