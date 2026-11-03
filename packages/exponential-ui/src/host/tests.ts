// VAPP-196 (eApps T1, spec docs/spec/apps.md §11): PACKAGE TESTS. A package
// carries `tests: [{name, records?, now?, as?, expr | query | action,
// expect}]`; `validatePackage` runs every one on an otherwise valid package,
// each against its OWN in-memory store seeded with `records`, on a fixed
// clock (`now`, UTC) as the member `as`. A failing test is an install issue
// at `/tests/<i>` (expected vs got), so the editor agent sees it like a type
// error. The evaluators are the ones every host runs: expressions (src/cel.ts),
// the records query (query.ts) and actions (actions.ts). The Rust core
// mirrors this file (`host::tests`); fixtures/package-tests.json locks it.

import { evaluateExpression, parseTimestamp } from "../cel"
import { applyRecordBatch, runAction } from "./actions"
import type { PackageIssue } from "./package"
import { kindTransitions } from "./record-transitions"
import { RECORDS_SCHEME, expandQuerySource, kindComputed, kindRefs, parseRecordsSource, recordsValue } from "./query"
import type { RecordRow } from "./query"
import { readableRows } from "./record-policy"
import { validateRecord } from "./records"
import type { RecordSchema } from "./records"
import { canonicalJson } from "./signature"
import { parseSource, sourceWithParams } from "./sources"

/** Caps: tests per package; the characters (code points) of a `got` an issue prints. */
export const PACKAGE_TEST_LIMITS = { tests: 50, gotMax: 300 } as const
/** The clock, the member and their team role a test runs at when it names none. */
export const PACKAGE_TEST_DEFAULTS = { now: `2026-01-01T00:00:00.000Z`, as: `member`, role: `member` } as const
/** eApps R3: the roles a test may run as. */
export const PACKAGE_TEST_ROLES: readonly string[] = [`member`, `owner`]
/** What a test runs: exactly one of these keys. */
export const PACKAGE_TEST_KINDS: readonly string[] = [`expr`, `query`, `action`]

export interface PackageTestResult {
  name: string
  pass: boolean
  /** What the test produced (`{error}` for a failure). */
  got: unknown
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)
const escape = (key: string) => key.replace(/~/g, `~0`).replace(/\//g, `~1`)
/** The seed row keys that are not data. */
const SEED_KEYS = [`id`, `author`]

/** Does `got` satisfy `expect`? An object matches by the keys it names
 *  (recursively), a list item by item with the same length, anything else
 *  by JSON equality. */
export function testMatches(expect: unknown, got: unknown): boolean {
  if (isObject(expect)) return isObject(got) && Object.keys(expect).every((k) => testMatches(expect[k], own(got, k) ? got[k] : null))
  if (Array.isArray(expect)) return Array.isArray(got) && got.length === expect.length && expect.every((e, i) => testMatches(e, got[i]))
  return (expect ?? null) === (got ?? null)
}

/** The shape issues of a package's `tests` (they run only without any). */
export function validatePackageTests(pkg: Record<string, unknown>): PackageIssue[] {
  const tests = pkg.tests
  if (tests === undefined) return []
  if (!Array.isArray(tests)) return [{ path: `/tests`, message: `tests is a list of {name, expr | query | action, expect}` }]
  const issues: PackageIssue[] = []
  if (tests.length > PACKAGE_TEST_LIMITS.tests) issues.push({ path: `/tests`, message: `at most ${PACKAGE_TEST_LIMITS.tests} tests` })
  const records = isObject(pkg.records) ? pkg.records : {}
  const queries = isObject(pkg.queries) ? pkg.queries : {}
  const actions = isObject(pkg.actions) ? pkg.actions : {}
  tests.forEach((t, i) => {
    const at = `/tests/${i}`
    if (!isObject(t)) return void issues.push({ path: at, message: `a test is {name, expr | query | action, expect}` })
    if (typeof t.name !== `string` || !t.name.trim()) issues.push({ path: `${at}/name`, message: `name is a non-empty string` })
    const kinds = PACKAGE_TEST_KINDS.filter((k) => t[k] !== undefined)
    if (kinds.length !== 1) issues.push({ path: at, message: `a test runs one of ${PACKAGE_TEST_KINDS.join(`, `)}` })
    if (!own(t, `expect`)) issues.push({ path: `${at}/expect`, message: `expect is required` })
    if (t.now !== undefined && (typeof t.now !== `string` || parseTimestamp(t.now) === null)) issues.push({ path: `${at}/now`, message: `now is an ISO timestamp` })
    if (t.as !== undefined && (typeof t.as !== `string` || !t.as)) issues.push({ path: `${at}/as`, message: `as is a member id` })
    if (t.role !== undefined && !PACKAGE_TEST_ROLES.includes(t.role as string)) issues.push({ path: `${at}/role`, message: `role is member or owner` })
    if (t.records !== undefined) {
      if (!isObject(t.records)) issues.push({ path: `${at}/records`, message: `records is {<kind>: [rows]}` })
      else
        for (const kind of Object.keys(t.records).sort()) {
          const kat = `${at}/records/${escape(kind)}`
          const def = own(records, kind) && isObject(records[kind]) ? (records[kind] as Record<string, unknown>) : null
          if (!def || !isObject(def.schema)) {
            issues.push({ path: kat, message: `records:${kind} names no declared kind` })
            continue
          }
          const rows = t.records[kind]
          if (!Array.isArray(rows)) {
            issues.push({ path: kat, message: `a kind's sample is a list of rows` })
            continue
          }
          rows.forEach((row, j) => {
            if (!isObject(row)) return void issues.push({ path: `${kat}/${j}`, message: `a row is an object` })
            for (const key of SEED_KEYS) if (row[key] !== undefined && (typeof row[key] !== `string` || !row[key])) issues.push({ path: `${kat}/${j}/${key}`, message: `${key} is a non-empty string` })
            for (const issue of validateRecord(def.schema as unknown as RecordSchema, seedData(row))) issues.push({ path: `${kat}/${j}${issue.path}`, message: issue.message })
          })
        }
    }
    if (kinds.length !== 1) return
    const kind = kinds[0]!
    const body = t[kind]
    const kat = `${at}/${kind}`
    if (kind === `expr`) {
      if (!isObject(body) || typeof body.expr !== `string`) issues.push({ path: `${kat}/expr`, message: `expr is a string` })
      else if (body.vars !== undefined && !isObject(body.vars)) issues.push({ path: `${kat}/vars`, message: `vars is an object` })
    } else if (kind === `query`) {
      const name = isObject(body) && typeof body.name === `string` ? body.name : ``
      if (!own(queries, name)) issues.push({ path: `${kat}/name`, message: `query ${name} names no package query` })
      if (isObject(body) && body.params !== undefined && !isObject(body.params)) issues.push({ path: `${kat}/params`, message: `params is an object` })
    } else {
      const name = isObject(body) && typeof body.name === `string` ? body.name : ``
      if (!own(actions, name)) issues.push({ path: `${kat}/name`, message: `action ${name} names no package action` })
    }
  })
  return issues
}

function seedData(row: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(row)) if (!SEED_KEYS.includes(k)) out[k] = v
  return out
}

/** A test's store: its `records`, ids `<kind>-<n>` unless given, authored
 *  by `author` (else the test's member), stamped `now`. */
function seedRows(records: unknown, me: string, now: string): RecordRow[] {
  const rows: RecordRow[] = []
  if (!isObject(records)) return rows
  for (const kind of Object.keys(records).sort()) {
    const list = records[kind]
    if (!Array.isArray(list)) continue
    list.forEach((row, j) => {
      if (!isObject(row)) return
      const author = typeof row.author === `string` ? row.author : me
      rows.push({ id: typeof row.id === `string` ? row.id : `${kind}-${j + 1}`, kind, data: seedData(row), createdBy: author, authorId: author, createdAt: now, updatedAt: now })
    })
  }
  return rows
}

/** A var of an `expr` test: `{source: "records:<kind>?…"}` = that binding's
 *  value over the test's store; anything else = the literal. */
function testVar(v: unknown, pkg: Record<string, unknown>, rows: RecordRow[], me: string, now: string): unknown {
  if (!isObject(v) || Object.keys(v).length !== 1 || typeof v.source !== `string`) return v
  return recordsSourceValue(v.source, pkg, rows, me, now)
}

function recordsSourceValue(source: string, pkg: Record<string, unknown>, rows: RecordRow[], me: string, now: string): unknown {
  const expanded = expandQuerySource(source, isObject(pkg.queries) ? pkg.queries : undefined)
  const parsed = parseSource(expanded)
  const query = parsed?.scheme === RECORDS_SCHEME ? parseRecordsSource(parsed) : null
  if (!query) return { error: `${source} is not a records source` }
  return recordsValue(rows, query, { me, now, computed: query.app === null ? kindComputed(pkg.records, query.from) : undefined, refs: query.app === null ? kindRefs(pkg.records, query.from) : undefined, transitions: query.app === null && query.from && isObject(pkg.records) ? kindTransitions(pkg.records[query.from]) : null }) ?? null
}

/** Run one test (a valid one): what it produced. */
function runTest(pkg: Record<string, unknown>, t: Record<string, unknown>): unknown {
  const now = typeof t.now === `string` ? t.now : PACKAGE_TEST_DEFAULTS.now
  const nowMs = parseTimestamp(now)!
  const me = typeof t.as === `string` ? t.as : PACKAGE_TEST_DEFAULTS.as
  const role = typeof t.role === `string` ? t.role : PACKAGE_TEST_DEFAULTS.role
  const kinds = isObject(pkg.records) ? pkg.records : undefined
  // eApps R3: the test reads what its member may read, and writes under the policy.
  const stored = seedRows(t.records, me, now)
  const rows = readableRows(stored, kinds, { id: me, role })
  if (isObject(t.expr)) {
    const vars: Record<string, unknown> = {}
    if (isObject(t.expr.vars)) for (const [k, v] of Object.entries(t.expr.vars)) vars[k] = testVar(v, pkg, rows, me, now)
    const result = evaluateExpression(t.expr.expr as string, vars, { now: nowMs })
    return `error` in result ? { error: result.error } : result.value
  }
  if (isObject(t.query)) {
    const name = t.query.name as string
    const q = (pkg.queries as Record<string, unknown>)[name]
    const from = isObject(q) && typeof q.from === `string` ? q.from : ``
    const params: Record<string, unknown> = {}
    if (isObject(t.query.params)) for (const [k, v] of Object.entries(t.query.params)) params[k] = typeof v === `string` ? v : canonicalJson(v)
    return recordsSourceValue(sourceWithParams(`${RECORDS_SCHEME}:${from}`, { query: name, ...params }), pkg, rows, me, now)
  }
  const a = t.action as Record<string, unknown>
  const result = runAction(pkg, a.name as string, a.params, { data: a.data, me, now: nowMs })
  if (`error` in result) return { error: result.error }
  const batch = applyRecordBatch(stored, result.plan.writes, { records: kinds, me, role, now, newId: (i) => `new-${i + 1}` })
  if (`error` in batch) return { error: batch.error }
  const records: Record<string, unknown[]> = {}
  for (const row of readableRows(batch.rows, kinds, { id: me, role })) (records[row.kind] ??= []).push({ ...row.data, id: row.id, author: row.authorId ?? row.createdBy })
  return { records, sets: result.plan.sets, calls: result.plan.calls }
}

/** Every test of a package, run (a package whose tests are malformed runs none). */
export function runPackageTests(pkg: Record<string, unknown>): PackageTestResult[] {
  if (!Array.isArray(pkg.tests) || validatePackageTests(pkg).length) return []
  return pkg.tests.map((t: Record<string, unknown>) => {
    const got = runTest(pkg, t)
    return { name: t.name as string, pass: testMatches(t.expect, got), got }
  })
}

/** The issue text of a failed test: expected vs got, both canonical JSON
 *  (got cut at `gotMax` code points). */
export function testFailure(name: string, expect: unknown, got: unknown): string {
  const g = Array.from(canonicalJson(got ?? null))
  const text = g.length > PACKAGE_TEST_LIMITS.gotMax ? `${g.slice(0, PACKAGE_TEST_LIMITS.gotMax).join(``)}…` : g.join(``)
  return `${name}: expected ${canonicalJson(expect ?? null)}, got ${text}`
}

/** The failing tests as install issues (`/tests/<i>`). */
export function packageTestIssues(pkg: Record<string, unknown>): PackageIssue[] {
  const tests = Array.isArray(pkg.tests) ? (pkg.tests as Record<string, unknown>[]) : []
  return runPackageTests(pkg).flatMap((r, i) => (r.pass ? [] : [{ path: `/tests/${i}`, message: testFailure(r.name, tests[i]!.expect, r.got) }]))
}
