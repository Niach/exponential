// eApps R8 (VAPP-233, docs/spec/apps.md §3.6/§2.3): declarative record
// migrations between package versions. A package declares
//
//   "migrations": [{ "from": "^1", "kind": "entry", "ops": [
//     { "rename": { "from": "issues", "to": "refs" } },
//     { "default": { "field": "project", "value": "Exponential" } },
//     { "drop": { "field": "legacy" } },
//     { "map": { "field": "hours", "expr": "minutes / 60.0" } },
//     { "renameKind": "worklog" } ] }]
//
// and a record written under version `v` of kind `k` runs, in order, every
// migration whose `from` range holds `v` and whose `kind` is the record's
// kind at that step (a `renameKind` moves the later steps to the new kind).
// A major bump of `version` that changes a published kind incompatibly
// without an op that covers each change is refused at upsert, naming the
// kind (`migrationRefusal`). Pure, mirrored line by line in the Rust core
// (`host::record_migrations`); fixtures/record-migrations.json is the spec.

import { evaluateExpression, expressionNames, parseExpression, type ExprNode } from "../cel"
import type { PackageIssue } from "./package"
import { RECORD_KIND_PATTERN } from "./records"

export type MigrationOp =
  | { rename: { from: string; to: string } }
  | { default: { field: string; value: unknown } }
  | { drop: { field: string } }
  | { map: { field: string; expr: string } }
  | { renameKind: string }

export interface RecordMigration {
  from: string
  kind: string
  ops: MigrationOp[]
}

/** The caps: migrations per package, ops per migration. */
export const RECORD_MIGRATIONS_MAX = 64
export const RECORD_MIGRATION_OPS_MAX = 32
export const MIGRATION_OP_NAMES: readonly string[] = [`rename`, `default`, `drop`, `map`, `renameKind`]

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/
const PARTIAL = /^(0|[1-9]\d*|x|\*)(?:\.(0|[1-9]\d*|x|\*))?(?:\.(0|[1-9]\d*|x|\*))?$/
const COMPARATOR = /^(>=|<=|>|<|=|\^|~)?(.+)$/

type Version = [number, number, number]
const versionOf = (v: string): Version | null => {
  const m = SEMVER.exec(v.trim())
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null
}
/** `1`, `1.2`, `1.2.3` (missing parts = 0): what an operator compares against. */
const NUMERIC = /^(0|[1-9]\d*)(?:\.(0|[1-9]\d*))?(?:\.(0|[1-9]\d*))?$/
const numericVersion = (v: string): Version | null => {
  const m = NUMERIC.exec(v)
  return m ? [Number(m[1]), Number(m[2] ?? 0), Number(m[3] ?? 0)] : null
}
const cmp = (a: Version, b: Version): number => a[0] - b[0] || a[1] - b[1] || a[2] - b[2]

/** One comparator of a range against a version; null = not a comparator. */
function comparatorHolds(token: string, v: Version): boolean | null {
  const m = COMPARATOR.exec(token)
  if (!m) return null
  const op = m[1] ?? ``
  const rest = m[2]!
  if (op === `` || op === `=`) {
    const p = PARTIAL.exec(rest)
    if (!p) return null
    const parts = [p[1], p[2], p[3]]
    return parts.every((part, i) => part === undefined || part === `x` || part === `*` || Number(part) === v[i])
  }
  const want = numericVersion(rest)
  if (!want) return null
  const c = cmp(v, want)
  if (op === `>=`) return c >= 0
  if (op === `>`) return c > 0
  if (op === `<=`) return c <= 0
  if (op === `<`) return c < 0
  if (op === `~`) return v[0] === want[0] && v[1] === want[1] && c >= 0
  // `^`: the same leftmost non-zero part, not lower.
  if (want[0] > 0) return v[0] === want[0] && c >= 0
  if (want[1] > 0) return v[0] === 0 && v[1] === want[1] && c >= 0
  return c === 0
}

/** A range's alternatives (`||`), each a list of comparators (AND); null = not a range. */
function rangeParts(range: string): string[][] | null {
  if (typeof range !== `string` || !range.trim()) return null
  const alts = range.split(`||`).map((a) => a.trim().split(/\s+/).filter(Boolean))
  if (alts.some((a) => !a.length)) return null
  for (const a of alts) for (const t of a) if (comparatorHolds(t, [0, 0, 0]) === null) return null
  return alts
}

/** Is `range` a SemVer range this rule reads: an exact or partial version
 *  (`1.2.3`, `1.x`, `1`, `*`), `^v`, `~v`, `>= > <= < v` comparators (v
 *  `1`, `1.2` or `1.2.3`, missing parts = 0)
 *  (space = and), alternatives with `||`. */
export const isVersionRange = (range: string): boolean => rangeParts(range) !== null

/** Does `version` (`MAJOR.MINOR.PATCH`) satisfy `range`? */
export function rangeHolds(version: string, range: string): boolean {
  const v = versionOf(version)
  const alts = rangeParts(range)
  if (!v || !alts) return false
  return alts.some((a) => a.every((t) => comparatorHolds(t, v) === true))
}

/** A package's migrations, the well-formed ones (bad entries dropped). */
export function packageMigrations(pkg: unknown): RecordMigration[] {
  const list = isObject(pkg) && Array.isArray(pkg.migrations) ? pkg.migrations : []
  return list.filter((m): m is RecordMigration => isObject(m) && typeof m.from === `string` && typeof m.kind === `string` && Array.isArray(m.ops))
}

const opName = (op: unknown): string | null => {
  if (!isObject(op)) return null
  const keys = Object.keys(op)
  return keys.length === 1 && MIGRATION_OP_NAMES.includes(keys[0]!) ? keys[0]! : null
}

export type MigrateResult = { kind: string; data: Record<string, unknown>; applied: number } | { error: string }

/** A record written under `version` as `kind` through `migrations`: every
 *  migration whose `from` holds `version` and whose `kind` is the record's
 *  kind at that step, its ops in order. `rename` moves a present field
 *  (overwriting the target); `default` sets a missing or null field; `drop`
 *  removes it; `map` writes a CEL expression over the record's fields (a
 *  name the record lacks reads null; `now` = the sweep's clock) into the
 *  field, an error fails the record (`migration <i> op <j>: …`);
 *  `renameKind` moves the record to another kind. `applied` = the
 *  migrations that ran (0 = untouched). */
export function migrateRecord(migrations: readonly RecordMigration[], version: string, kind: string, data: Record<string, unknown>, now = 0): MigrateResult {
  let k = kind
  const d: Record<string, unknown> = { ...data }
  let applied = 0
  for (const [i, m] of migrations.entries()) {
    if (m.kind !== k || !rangeHolds(version, m.from)) continue
    applied += 1
    for (const [j, op] of m.ops.entries()) {
      const at = `migration ${i} op ${j}`
      const o = op as Record<string, unknown>
      switch (opName(op)) {
        case `rename`: {
          const r = o.rename as { from: string; to: string }
          if (own(d, r.from)) {
            const value = d[r.from]
            delete d[r.from]
            d[r.to] = value
          }
          break
        }
        case `default`: {
          const r = o.default as { field: string; value: unknown }
          if (d[r.field] === undefined || d[r.field] === null) d[r.field] = r.value
          break
        }
        case `drop`:
          delete d[(o.drop as { field: string }).field]
          break
        case `map`: {
          const r = o.map as { field: string; expr: string }
          const parsed = parseExpression(r.expr)
          if (`error` in parsed) return { error: `${at}: ${parsed.error}` }
          const vars: Record<string, unknown> = { ...d }
          for (const name of expressionNames(parsed.ast as ExprNode)) if (!own(vars, name)) vars[name] = null
          const result = evaluateExpression(parsed.ast as ExprNode, vars, { now })
          if (`error` in result) return { error: `${at}: ${result.error}` }
          d[r.field] = result.value
          break
        }
        case `renameKind`:
          k = o.renameKind as string
          break
        default:
          return { error: `${at}: unknown op` }
      }
    }
  }
  return { kind: k, data: d, applied }
}

/** eApps R8: the issues of a package's `migrations` (each under
 *  `/migrations/<i>`): a list of at most RECORD_MIGRATIONS_MAX `{from,
 *  kind, ops}`; `from` a version range; `kind` a kind name; `ops` at most
 *  RECORD_MIGRATION_OPS_MAX, each ONE of rename {from, to}, default {field,
 *  value}, drop {field}, map {field, expr} (the expression parses),
 *  renameKind <kind>; the kind a migration ends in is a declared kind. */
export function validateMigrations(pkg: unknown): PackageIssue[] {
  if (!isObject(pkg) || pkg.migrations === undefined) return []
  const list = pkg.migrations
  if (!Array.isArray(list)) return [{ path: `/migrations`, message: `migrations is a list of {from, kind, ops}` }]
  const issues: PackageIssue[] = []
  if (list.length > RECORD_MIGRATIONS_MAX) issues.push({ path: `/migrations`, message: `at most ${RECORD_MIGRATIONS_MAX} migrations` })
  const records = isObject(pkg.records) ? pkg.records : {}
  const field = (v: unknown) => typeof v === `string` && v.length > 0
  list.forEach((m, i) => {
    const at = `/migrations/${i}`
    if (!isObject(m)) return void issues.push({ path: at, message: `a migration is {from, kind, ops}` })
    for (const key of Object.keys(m).sort()) if (![`from`, `kind`, `ops`].includes(key)) issues.push({ path: `${at}/${key}`, message: `unknown key ${key}` })
    if (typeof m.from !== `string` || !isVersionRange(m.from)) issues.push({ path: `${at}/from`, message: `from is a version or a range (1.2.3, 1.x, ^1.2.0, >=1.0.0 <2.0.0)` })
    let kind = typeof m.kind === `string` && RECORD_KIND_PATTERN.test(m.kind) ? m.kind : null
    if (kind === null) issues.push({ path: `${at}/kind`, message: `kind is a kind name` })
    if (!Array.isArray(m.ops)) return void issues.push({ path: `${at}/ops`, message: `ops is a list` })
    if (m.ops.length > RECORD_MIGRATION_OPS_MAX) issues.push({ path: `${at}/ops`, message: `at most ${RECORD_MIGRATION_OPS_MAX} ops` })
    m.ops.forEach((op, j) => {
      const op_at = `${at}/ops/${j}`
      const name = opName(op)
      if (!name) return void issues.push({ path: op_at, message: `an op is one of ${MIGRATION_OP_NAMES.join(`, `)}` })
      const arg = (op as Record<string, unknown>)[name]
      const bad = (shape: string) => issues.push({ path: `${op_at}/${name}`, message: `${name} is ${shape}` })
      if (name === `rename` && !(isObject(arg) && field(arg.from) && field(arg.to) && Object.keys(arg).length === 2)) bad(`{from, to}`)
      else if (name === `default` && !(isObject(arg) && field(arg.field) && own(arg, `value`) && Object.keys(arg).length === 2)) bad(`{field, value}`)
      else if (name === `drop` && !(isObject(arg) && field(arg.field) && Object.keys(arg).length === 1)) bad(`{field}`)
      else if (name === `map`) {
        if (!(isObject(arg) && field(arg.field) && typeof arg.expr === `string` && Object.keys(arg).length === 2)) bad(`{field, expr}`)
        else {
          const parsed = parseExpression(arg.expr as string)
          if (`error` in parsed) issues.push({ path: `${op_at}/map/expr`, message: parsed.error })
        }
      } else if (name === `renameKind`) {
        if (typeof arg !== `string` || !RECORD_KIND_PATTERN.test(arg)) bad(`a kind name`)
        else kind = arg
      }
    })
    if (kind !== null && !own(records, kind)) issues.push({ path: `${at}/kind`, message: `the migration ends in ${kind}, which the package does not declare` })
  })
  return issues
}

/** One incompatible change of a kind between two packages. */
export interface KindChange {
  /** `removed` (the kind), `drops` / `retypes` / `narrows` / `requires` (a field). */
  change: `removed` | `drops` | `retypes` | `narrows` | `requires`
  field: string | null
}

const propsOf = (kind: unknown): Record<string, Record<string, unknown>> =>
  isObject(kind) && isObject(kind.schema) && isObject(kind.schema.properties) ? (kind.schema.properties as Record<string, Record<string, unknown>>) : {}
const requiredOf = (kind: unknown): string[] => (isObject(kind) && isObject(kind.schema) && Array.isArray(kind.schema.required) ? (kind.schema.required as unknown[]).filter((s): s is string => typeof s === `string`) : [])
const typeOf = (f: Record<string, unknown> | undefined) => (f ? `${String(f.type)}${f.type === `array` && isObject(f.items) ? `<${String(f.items.type)}>` : ``}` : ``)
const enumOf = (f: Record<string, unknown> | undefined): string[] | null => {
  const e = f?.type === `array` && isObject(f.items) ? f.items.enum : f?.enum
  return Array.isArray(e) ? (e as string[]) : null
}

/** The incompatible changes of each kind from `before` to `after` (kinds
 *  and fields in code-unit order): a kind gone, a field gone, a field's type
 *  changed, an enum that lost a value, a field required now that was not.
 *  An added optional field (or kind) is compatible. */
export function kindChanges(before: unknown, after: unknown): Record<string, KindChange[]> {
  const out: Record<string, KindChange[]> = {}
  const b = isObject(before) && isObject(before.records) ? before.records : {}
  const a = isObject(after) && isObject(after.records) ? after.records : {}
  for (const kind of Object.keys(b).sort()) {
    const changes: KindChange[] = []
    if (!own(a, kind)) changes.push({ change: `removed`, field: null })
    else {
      const bp = propsOf(b[kind])
      const ap = propsOf(a[kind])
      const breq = requiredOf(b[kind])
      for (const f of Object.keys(bp).sort()) {
        if (!own(ap, f)) changes.push({ change: `drops`, field: f })
        else if (typeOf(bp[f]) !== typeOf(ap[f])) changes.push({ change: `retypes`, field: f })
        else {
          const be = enumOf(bp[f])
          const ae = enumOf(ap[f])
          if (ae && (!be || be.some((v) => !ae.includes(v)))) changes.push({ change: `narrows`, field: f })
        }
      }
      for (const f of [...requiredOf(a[kind])].sort()) if (!breq.includes(f) && own(ap, f) && !changes.some((c) => c.field === f)) changes.push({ change: `requires`, field: f })
    }
    if (changes.length) out[kind] = changes
  }
  return out
}

/** Does an op of the migrations from `version` for `kind` cover `c`?
 *  removed = a renameKind; drops = a drop or a rename from the field;
 *  retypes / narrows = a map of the field; requires = a default, a map or a
 *  rename to the field. */
function covered(migrations: readonly RecordMigration[], version: string, kind: string, c: KindChange): boolean {
  for (const m of migrations) {
    if (m.kind !== kind || !rangeHolds(version, m.from)) continue
    for (const op of m.ops) {
      const o = op as Record<string, Record<string, unknown> | string>
      const name = opName(op)
      if (c.change === `removed` && name === `renameKind`) return true
      if (c.change === `drops` && ((name === `drop` && (o.drop as Record<string, unknown>).field === c.field) || (name === `rename` && (o.rename as Record<string, unknown>).from === c.field))) return true
      if ((c.change === `retypes` || c.change === `narrows`) && name === `map` && (o.map as Record<string, unknown>).field === c.field) return true
      if (c.change === `requires` && ((name === `default` && (o.default as Record<string, unknown>).field === c.field) || (name === `map` && (o.map as Record<string, unknown>).field === c.field) || (name === `rename` && (o.rename as Record<string, unknown>).to === c.field)))
        return true
    }
  }
  return false
}

const CHANGE_TEXT: Record<KindChange[`change`], string> = { removed: `is removed`, drops: `drops`, retypes: `changes the type of`, narrows: `narrows the values of`, requires: `requires` }

/** Why `after` may not replace `before` (the published package): a MAJOR
 *  bump of `version` whose kinds change incompatibly where no migration from
 *  `before.version` covers the change; null = it may (no major bump, no
 *  incompatible change, or every change covered). The first kind and change
 *  in code-unit order: `entry drops note in 2.0.0: add a migration from 1.4.0 that covers it`. */
export function migrationRefusal(before: unknown, after: unknown): string | null {
  const bv = isObject(before) && typeof before.version === `string` ? versionOf(before.version) : null
  const av = isObject(after) && typeof after.version === `string` ? versionOf(after.version) : null
  if (!bv || !av || av[0] <= bv[0]) return null
  const migrations = packageMigrations(after)
  const changes = kindChanges(before, after)
  for (const kind of Object.keys(changes).sort()) {
    for (const c of changes[kind]!) {
      if (covered(migrations, (before as { version: string }).version, kind, c)) continue
      const what = c.field === null ? `${kind} ${CHANGE_TEXT[c.change]}` : `${kind} ${CHANGE_TEXT[c.change]} ${c.field}`
      return `${what} in ${(after as { version: string }).version}: add a migration from ${(before as { version: string }).version} that covers it`
    }
  }
  return null
}
