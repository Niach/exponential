// EXP-1490: an app's MCP tools, declared as bindings (`exposes`). Pure and
// byte-locked by `fixtures/exposes.json` (its `$comment` is the spec); the
// server broker (`apps/web/src/lib/emcp`, #EXP-1491) and the Rust crate
// `emcp` (#EXP-1492) replay the same cases.

import { actionOwnerOnly, callDestructive, parseActionName } from "@exp/domain-contract/app-actions"
import { createHash } from "node:crypto"

import fixture from "../fixtures/exposes.json" with { type: "json" }

export const EXPOSES_LIMITS = fixture.limits
export const EXPOSES_ERRORS = fixture.errors
export const PROVENANCE_KEYS = fixture.provenance

export type ToolAccess = `read` | `write`

export type RecordsRun = {
  records: {
    op: `list` | `get` | `upsert`
    kind: string
    id?: unknown
    where?: Record<string, unknown>
    data?: Record<string, unknown>
    limit?: unknown
  }
}
export type ActionRun = { action: string; args?: Record<string, unknown> }
export type UpstreamRun = { upstream: string; tool: string; args?: Record<string, unknown> }
export type ExposeRun = RecordsRun | ActionRun | UpstreamRun

export interface Expose {
  name: string
  title: string
  description: string
  /** A JSON Schema of `type: "object"`. */
  input: { type: `object`; properties?: Record<string, unknown>; required?: string[] }
  access: ToolAccess
  run: ExposeRun
}

/** What the app's package declares beside its `exposes`. */
export interface ExposeContext {
  /** The record kinds of the manifest (`records: {<kind>: …}`). */
  kinds: readonly string[]
  /** The team MCP server names the app declares (`upstreams`): an app sees
   *  only its own. */
  upstreams: readonly string[]
}

export type RunVia = `records` | `action` | `upstream`

export function runVia(run: ExposeRun): RunVia {
  if (`records` in run) return `records`
  if (`action` in run) return `action`
  return `upstream`
}

const READ_RECORD_OPS: readonly string[] = fixture.readRecordOps
const RECORD_OPS: readonly string[] = fixture.recordOps
const READ_OP_WORDS: readonly string[] = fixture.readOpWords
const DESTRUCTIVE_WORDS: readonly string[] = fixture.destructiveWords

/** An `exponential.<noun>.<op>` action is a read when the last word of its
 *  op is a read word (`list`, `get`, …); a verb tool is a write. */
export function actionAccess(name: string): ToolAccess {
  const target = parseActionName(name)
  const last = target?.op?.split(`_`).at(-1)
  return last !== undefined && READ_OP_WORDS.includes(last) ? `read` : `write`
}

/** The MCP annotations of an upstream tool (from its `tools/list`). */
export interface UpstreamAnnotations {
  readOnlyHint?: boolean
  destructiveHint?: boolean
}

/** An upstream tool is destructive when it says so (`destructiveHint:
 *  true`) or a word of its name is a destructive word. */
export function upstreamDestructive(tool: string, annotations: UpstreamAnnotations | null | undefined): boolean {
  if (annotations?.destructiveHint === true) return true
  return tool.toLowerCase().split(/[^a-z]+/).some((word) => DESTRUCTIVE_WORDS.includes(word))
}

/** An upstream tool reads only when it says so (`readOnlyHint: true`). */
export function upstreamAccess(annotations: UpstreamAnnotations | null | undefined): ToolAccess {
  return annotations?.readOnlyHint === true ? `read` : `write`
}

const TEMPLATE = /\$\{input\.([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)\}/g
const WHOLE_TEMPLATE = /^\$\{input\.([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)\}$/

/** The input paths a value's templates name (`${input.a.b}` → `a.b`). */
export function templatePaths(value: unknown): string[] {
  const out: string[] = []
  const walk = (v: unknown) => {
    if (typeof v === `string`) {
      for (const m of v.matchAll(TEMPLATE)) out.push(m[1]!)
    } else if (Array.isArray(v)) {
      v.forEach(walk)
    } else if (v !== null && typeof v === `object`) {
      Object.values(v).forEach(walk)
    }
  }
  walk(value)
  return out
}

function lookup(input: unknown, path: string): unknown {
  let at: unknown = input
  for (const key of path.split(`.`)) {
    if (at === null || typeof at !== `object` || Array.isArray(at) || !Object.prototype.hasOwnProperty.call(at, key)) return undefined
    at = (at as Record<string, unknown>)[key]
  }
  return at
}

function stringify(value: unknown): string {
  if (value === undefined || value === null) return ``
  if (typeof value === `string`) return value
  if (typeof value === `number` || typeof value === `boolean`) return String(value)
  return JSON.stringify(value)
}

/** Fill a binding's args from the call's input. A string that is exactly
 *  one template takes the value itself (its type kept); an absent value
 *  drops the object key and is null in an array. A template inside other
 *  text is replaced by the value as text (absent/null = empty, objects =
 *  compact JSON). Everything else is copied. */
export function resolveArgs(template: unknown, input: Record<string, unknown>): unknown {
  if (typeof template === `string`) {
    const whole = template.match(WHOLE_TEMPLATE)
    if (whole) return lookup(input, whole[1]!)
    return template.replace(TEMPLATE, (_, path: string) => stringify(lookup(input, path)))
  }
  if (Array.isArray(template)) {
    return template.map((item) => {
      const value = resolveArgs(item, input)
      return value === undefined ? null : value
    })
  }
  if (template !== null && typeof template === `object`) {
    const out: Record<string, unknown> = {}
    for (const [key, item] of Object.entries(template)) {
      const value = resolveArgs(item, input)
      if (value !== undefined) out[key] = value
    }
    return out
  }
  return template
}

export interface ExposeError {
  /** The binding's index in `exposes`, null for the list itself. */
  index: number | null
  code: string
}

const NAME = new RegExp(fixture.limits.namePattern)

/** Every error of an app's `exposes`, in order (empty = valid). */
export function validateExposes(exposes: readonly Expose[], ctx: ExposeContext): ExposeError[] {
  const errors: ExposeError[] = []
  if (exposes.length > fixture.limits.maxTools) errors.push({ index: null, code: `tooMany` })
  const seen = new Set<string>()
  exposes.forEach((expose, index) => {
    const fail = (code: string) => errors.push({ index, code })
    if (typeof expose.name !== `string` || !NAME.test(expose.name) || expose.name.length > fixture.limits.nameMax || expose.name.includes(`__`)) fail(`name`)
    else if (seen.has(expose.name)) fail(`duplicate`)
    else seen.add(expose.name)
    if (typeof expose.title !== `string` || expose.title.trim() === `` || expose.title.length > fixture.limits.titleMax) fail(`title`)
    if (typeof expose.description !== `string` || expose.description.trim() === `` || expose.description.length > fixture.limits.descriptionMax) fail(`description`)
    if (expose.input === null || typeof expose.input !== `object` || expose.input.type !== `object`) fail(`input`)
    if (expose.access !== `read` && expose.access !== `write`) fail(`access`)
    const properties = expose.input?.properties ?? {}
    const run = expose.run as unknown
    if (run === null || typeof run !== `object`) return fail(`run`)
    const keys = [`records`, `action`, `upstream`].filter((k) => k in run)
    if (keys.length !== 1) return fail(`run`)
    for (const path of templatePaths(run)) {
      if (!Object.prototype.hasOwnProperty.call(properties, path.split(`.`)[0]!)) fail(`template`)
    }
    const via = runVia(expose.run)
    let level: ToolAccess
    if (via === `records`) {
      const records = (expose.run as RecordsRun).records
      const op: string | undefined = records?.op
      if (op === undefined || !RECORD_OPS.includes(op)) return fail(op === `delete` ? `destructive` : `run`)
      if (!ctx.kinds.includes(records.kind)) fail(`kind`)
      level = READ_RECORD_OPS.includes(records.op) ? `read` : `write`
    } else if (via === `action`) {
      const name = (expose.run as ActionRun).action
      if (parseActionName(name) === null) return fail(`action`)
      if (callDestructive(name, (expose.run as ActionRun).args)) return fail(`destructive`)
      if (actionOwnerOnly(name)) return fail(`ownerOnly`)
      level = actionAccess(name)
    } else {
      const upstream = expose.run as UpstreamRun
      if (!ctx.upstreams.includes(upstream.upstream)) fail(`upstream`)
      if (typeof upstream.tool !== `string` || upstream.tool === ``) return fail(`run`)
      if (upstreamDestructive(upstream.tool, null)) return fail(`destructive`)
      // An upstream's level is checked again on its live annotations.
      return
    }
    if (expose.access === `read` && level === `write`) fail(`accessTooLow`)
  })
  return errors
}

/** The call-time check of an upstream binding against the upstream tool's
 *  live annotations: null = callable, else the error code. */
export function upstreamCallError(expose: Expose, annotations: UpstreamAnnotations | null | undefined): string | null {
  const run = expose.run as UpstreamRun
  if (upstreamDestructive(run.tool, annotations)) return `destructive`
  if (expose.access === `read` && upstreamAccess(annotations) === `write`) return `accessTooLow`
  return null
}

const BASE32 = `abcdefghijklmnopqrstuvwxyz234567`

/** `n` lowercase base32 characters of the SHA-256 of `text`. */
export function shortHash(text: string, n: number = fixture.limits.slugHashLength): string {
  const bytes = createHash(`sha256`).update(text, `utf8`).digest()
  let bits = 0
  let value = 0
  let out = ``
  for (const byte of bytes) {
    value = (value << 8) | byte
    bits += 8
    while (bits >= 5 && out.length < n) {
      out += BASE32[(value >>> (bits - 5)) & 31]
      bits -= 5
    }
    value &= (1 << bits) - 1
    if (out.length >= n) break
  }
  return out
}

/** The MCP name of an app's tool: `<slug>__<name>`. Longer than
 *  `toolNameMax`, the slug is cut and ends in `-<hash of the full slug>`
 *  so the name stays unique and stable. */
export function qualifiedToolName(slug: string, name: string): string {
  const max = fixture.limits.toolNameMax
  const full = `${slug}${fixture.limits.separator}${name}`
  if (full.length <= max) return full
  const hash = shortHash(slug)
  const keep = max - fixture.limits.separator.length - name.length - hash.length - 1
  const cut = slug.slice(0, Math.max(0, keep)).replace(/-+$/, ``)
  return `${cut}-${hash}${fixture.limits.separator}${name}`
}

/** `<slug>__<name>` back to its parts (the first `__` splits: a binding
 *  name never holds one); null for anything else. */
export function splitToolName(qualified: string): { app: string; tool: string } | null {
  const at = qualified.indexOf(fixture.limits.separator)
  if (at <= 0) return null
  const tool = qualified.slice(at + fixture.limits.separator.length)
  if (tool === `` || tool.includes(fixture.limits.separator)) return null
  return { app: qualified.slice(0, at), tool }
}

export interface ListedApp {
  slug: string
  exposes: readonly { name: string }[]
}

/** The tool names of several apps on one endpoint, apps sorted by slug. A
 *  name an earlier app already holds is dropped (only a cut slug can
 *  collide). */
export function listToolNames(apps: readonly ListedApp[]): { app: string; tool: string; name: string }[] {
  const out: { app: string; tool: string; name: string }[] = []
  const taken = new Set<string>()
  for (const app of [...apps].sort((a, b) => (a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0))) {
    for (const expose of app.exposes) {
      const name = qualifiedToolName(app.slug, expose.name)
      if (taken.has(name)) continue
      taken.add(name)
      out.push({ app: app.slug, tool: expose.name, name })
    }
  }
  return out
}

/** The `_meta` every result of an app tool carries (provenance: which app
 *  and binding answered, and through what). */
export function provenanceMeta(slug: string, expose: Expose): Record<string, string> {
  const keys = fixture.provenance
  const meta: Record<string, string> = {
    [keys.app]: slug,
    [keys.tool]: expose.name,
    [keys.via]: runVia(expose.run),
  }
  if (`upstream` in expose.run) meta[keys.upstream] = expose.run.upstream
  return meta
}
