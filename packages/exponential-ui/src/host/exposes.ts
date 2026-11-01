// VAPP-180: the MCP tools a package EXPOSES (docs/package-format.md
// "Exposes"): declarative bindings `{name, title, description, input, access,
// run}` a host's MCP broker lists and runs as the calling person, never code.
// The format half of the eMCP contract (packages/emcp `fixtures/exposes.json`
// in the Exponential repo, replayed by `exposes.test.ts`): what any host can
// check from the package alone. What a run may reach on a given host (which
// actions exist, which are owner-only or destructive by their arguments,
// which upstream servers the team has) is that host's own check. Pure, so the
// Rust core mirrors it (`host/exposes.rs`); fixtures/host-router.json
// (validation) replays it.

import host from "../../catalog/host.json" with { type: "json" }
import type { PackageIssue } from "./package"

export const EXPOSES_LIMITS = host.package.exposes.limits
export const EXPOSE_RUN_KINDS: readonly string[] = host.package.exposes.runKinds
const RECORD_OPS: readonly string[] = host.package.exposes.recordOps
const READ_RECORD_OPS: readonly string[] = host.package.exposes.readRecordOps
const READ_OP_WORDS: readonly string[] = host.package.exposes.readOpWords
const DESTRUCTIVE_WORDS: readonly string[] = host.package.exposes.destructiveWords

export type ExposeAccess = `read` | `write`

export interface PackageExpose {
  /** `^[a-z][a-z0-9_]*$`, at most `limits.nameMax`, never `__`. */
  name: string
  title: string
  description: string
  /** A JSON Schema of `type: "object"`: the tool's inputSchema. */
  input: { type: `object`; properties?: Record<string, unknown>; required?: string[] }
  access: ExposeAccess
  /** Exactly one of `records`, `action`, `upstream`; every string may hold
   *  `${input.<path>}` templates. */
  run:
    | { records: { op: `list` | `get` | `upsert`; kind: string; id?: unknown; where?: Record<string, unknown>; data?: Record<string, unknown>; limit?: unknown } }
    | { action: string; args?: Record<string, unknown> }
    | { upstream: string; tool: string; args?: Record<string, unknown> }
}

/** One problem with a binding: the eMCP error `code` (`fixtures/exposes.json`
 *  `errors`), the binding's `index` (null = the list) and the package issue. */
export interface ExposeIssue extends PackageIssue {
  index: number | null
  code: string
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const NAME = new RegExp(EXPOSES_LIMITS.namePattern)
const ACTION = new RegExp(host.package.exposes.actionPattern)
const TEMPLATE = /\$\{input\.([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)\}/g

/** The input paths a run's templates name (`${input.a.b}` → `a.b`), in
 *  document order. */
export function exposeTemplatePaths(value: unknown): string[] {
  const out: string[] = []
  const walk = (v: unknown) => {
    if (typeof v === `string`) for (const m of v.matchAll(TEMPLATE)) out.push(m[1]!)
    else if (Array.isArray(v)) v.forEach(walk)
    else if (isObject(v)) Object.values(v).forEach(walk)
  }
  walk(value)
  return out
}

/** The words of a function or tool name (`issues.update_status` → issues,
 *  update, status). */
const words = (name: string): string[] => name.toLowerCase().split(/[^a-z]+/).filter(Boolean)

/** An action reads when the last word of its name is a read word
 *  (`exponential.issues.list`); a verb writes. */
export function exposeActionAccess(action: string): ExposeAccess {
  const last = words(action.split(`.`).at(-1) ?? ``).at(-1)
  return last !== undefined && READ_OP_WORDS.includes(last) ? `read` : `write`
}

/** A tool or action name holding a destructive word (delete, merge, close,
 *  end, remove, trash) is never exposed. */
export function exposeNameDestructive(name: string): boolean {
  return words(name).some((word) => DESTRUCTIVE_WORDS.includes(word))
}

/** The package's declared upstreams (valid entries only). */
function upstreamNames(upstreams: unknown): string[] {
  return Array.isArray(upstreams) ? upstreams.filter((u): u is string => typeof u === `string` && u !== ``) : []
}

/** Every problem with a package's `upstreams`: the team MCP server names
 *  its upstream bindings may call. */
export function validateUpstreams(upstreams: unknown): PackageIssue[] {
  if (!Array.isArray(upstreams) || upstreams.some((u) => typeof u !== `string` || u === ``)) return [{ path: `/upstreams`, message: `upstreams is a list of MCP server names` }]
  const issues: PackageIssue[] = []
  upstreams.forEach((name, i) => {
    if (upstreams.indexOf(name) !== i) issues.push({ path: `/upstreams/${i}`, message: `duplicate upstream ${name}` })
  })
  return issues
}

/** Every problem with a package's `exposes`, in binding order, with its
 *  eMCP code. `records` and `upstreams` = the package's (a binding reads
 *  only its own kinds and calls only its own upstreams). */
export function exposeIssues(exposes: unknown, records: unknown, upstreams: unknown): ExposeIssue[] {
  if (!Array.isArray(exposes)) return [{ index: null, code: `run`, path: `/exposes`, message: `exposes is a list of tools` }]
  const issues: ExposeIssue[] = []
  if (exposes.length > EXPOSES_LIMITS.maxTools) issues.push({ index: null, code: `tooMany`, path: `/exposes`, message: `exposes lists at most ${EXPOSES_LIMITS.maxTools} tools` })
  const kinds = isObject(records) ? Object.keys(records) : []
  const servers = upstreamNames(upstreams)
  const seen = new Set<string>()
  exposes.forEach((expose, index) => {
    const at = `/exposes/${index}`
    const fail = (code: string, path: string, message: string) => void issues.push({ index, code, path: `${at}${path}`, message })
    if (!isObject(expose)) return fail(`run`, ``, `a tool is {name, title, description, input, access, run}`)
    const name = expose.name
    if (typeof name !== `string` || !NAME.test(name) || name.length > EXPOSES_LIMITS.nameMax || name.includes(EXPOSES_LIMITS.separator))
      fail(`name`, `/name`, `name is lowercase letters, digits and _ (at most ${EXPOSES_LIMITS.nameMax}, no ${EXPOSES_LIMITS.separator})`)
    else if (seen.has(name)) fail(`duplicate`, `/name`, `duplicate tool ${name}`)
    else seen.add(name)
    if (typeof expose.title !== `string` || !expose.title.trim() || expose.title.length > EXPOSES_LIMITS.titleMax)
      fail(`title`, `/title`, `title is a non-blank string of at most ${EXPOSES_LIMITS.titleMax} characters`)
    if (typeof expose.description !== `string` || !expose.description.trim() || expose.description.length > EXPOSES_LIMITS.descriptionMax)
      fail(`description`, `/description`, `description is a non-blank string of at most ${EXPOSES_LIMITS.descriptionMax} characters`)
    const input = expose.input
    if (!isObject(input) || input.type !== `object` || (input.properties !== undefined && !isObject(input.properties)))
      fail(`input`, `/input`, `input is a JSON Schema of type object`)
    if (expose.access !== `read` && expose.access !== `write`) fail(`access`, `/access`, `access is read or write`)
    const properties = isObject(input) && isObject(input.properties) ? input.properties : {}
    const run = expose.run
    if (!isObject(run) || EXPOSE_RUN_KINDS.filter((k) => k in run).length !== 1) return fail(`run`, `/run`, `run holds exactly one of records, action, upstream`)
    for (const path of exposeTemplatePaths(run))
      if (!Object.prototype.hasOwnProperty.call(properties, path.split(`.`)[0]!)) fail(`template`, `/run`, `\${input.${path}} names no input property`)
    let level: ExposeAccess
    if (`records` in run) {
      const records = run.records
      const op = isObject(records) ? records.op : undefined
      if (typeof op !== `string` || !RECORD_OPS.includes(op))
        return op === `delete` ? fail(`destructive`, `/run/records/op`, `a records delete is destructive: never exposed`) : fail(`run`, `/run/records/op`, `op is ${RECORD_OPS.join(`, `)}`)
      if (typeof (records as Record<string, unknown>).kind !== `string` || !kinds.includes((records as Record<string, string>).kind))
        fail(`kind`, `/run/records/kind`, `kind names no declared record kind`)
      level = READ_RECORD_OPS.includes(op) ? `read` : `write`
    } else if (`action` in run) {
      const action = run.action
      if (typeof action !== `string` || !ACTION.test(action)) return fail(`action`, `/run/action`, `action is a function name like namespace.function`)
      if (exposeNameDestructive(action)) return fail(`destructive`, `/run/action`, `a destructive action is never exposed`)
      level = exposeActionAccess(action)
    } else {
      if (typeof run.upstream !== `string` || !servers.includes(run.upstream)) fail(`upstream`, `/run/upstream`, `upstream names none of the package's upstreams`)
      if (typeof run.tool !== `string` || run.tool === ``) return fail(`run`, `/run/tool`, `tool is a non-empty string`)
      if (exposeNameDestructive(run.tool)) return fail(`destructive`, `/run/tool`, `a destructive tool is never exposed`)
      // An upstream tool's level is its live annotations': the host checks it at call.
      return
    }
    if (expose.access === `read` && level === `write`) fail(`accessTooLow`, `/access`, `the run writes: access is write`)
  })
  return issues
}

/** Every problem with a package's `exposes` (`exposeIssues` as package
 *  issues). */
export function validatePackageExposes(exposes: unknown, records: unknown, upstreams: unknown): PackageIssue[] {
  return exposeIssues(exposes, records, upstreams).map(({ path, message }) => ({ path, message }))
}
