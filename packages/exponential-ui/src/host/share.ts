// VAPP-241 (eApps P1, docs/package-format.md "Public links",
// fixtures/package-share.json): a package's PUBLIC LINK definitions. Only the
// declaration lives here; the links themselves (tokens, caps, the guest pages)
// are the host's. Pure; the Rust core mirrors it (host::share).

import { validatePackageExpressions } from "./expressions"
import type { PackageIssue } from "./package"
import { RECORDS_SCHEME } from "./query"
import { parseSource } from "./sources"

export const MAX_SHARES = 5
export const SHARE_NAME_PATTERN = /^[a-z][a-z0-9_]{0,23}$/
export const SHARE_MODES: readonly string[] = [`submit`, `view`]
export const MAX_SHARE_EXPIRES_DAYS = 3650

export interface PackageShare {
  name: string
  mode: `submit` | `view`
  kind: string
  /** The record fields it exposes, in order; default every field but files. */
  fields?: string[]
  /** `view`: a named package query of the kind. */
  query?: string
  /** VAPP-246, `view`: a package template the guest page renders over the
   *  snapshot; default a read-only RecordTable. */
  template?: string
  /** Days from enabling until the link stops; default never. */
  expires?: number
}

const SHARE_KEYS = [`name`, `mode`, `kind`, `fields`, `query`, `template`, `expires`]
const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)

/** VAPP-246: a template as a guest's host gets it: every read is the link's
 *  one snapshot, so no binding or RecordTable names a query (the package's
 *  queries never reach a guest). `validatePackageShares` vouches for the rest. */
export function guestTemplate(template: Record<string, unknown>, kind: string): Record<string, unknown> {
  const out: Record<string, unknown> = { ...template }
  if (Array.isArray(template.bindings)) out.bindings = template.bindings.map((b) => (isObject(b) ? { ...b, source: `${RECORDS_SCHEME}:${kind}` } : b))
  if (Array.isArray(template.components))
    out.components = template.components.map((c) => {
      if (!isObject(c) || c.component !== `RecordTable`) return c
      const { query: _query, ...table } = c
      return table
    })
  return out
}

/** Does this source read the link's own snapshot: its kind, and at most the link's query? */
function readsSnapshot(source: string, kind: unknown, query: unknown): boolean {
  const parsed = parseSource(source)
  if (!parsed || parsed.scheme !== RECORDS_SCHEME || parsed.name !== kind) return false
  const keys = Object.keys(parsed.params)
  return keys.length === 0 || (keys.length === 1 && parsed.params.query !== undefined && parsed.params.query === query)
}

/** The first `exp:` string anywhere below a value. */
function expSource(value: unknown): string | null {
  if (typeof value === `string`) return value.toLowerCase().startsWith(`exp:`) ? value : null
  for (const v of Array.isArray(value) ? value : isObject(value) ? Object.values(value) : []) {
    const hit = expSource(v)
    if (hit !== null) return hit
  }
  return null
}

const GUEST_CALLS: readonly string[] = [`set`, `openUrl`]

/** VAPP-246: a view link's `template`: a guest's host has one source (the
 *  snapshot, narrowed to the link's fields), no function but `set` and
 *  `openUrl`, no extension and no `exp:` source, and refuses a package outside
 *  that, so install does first. Each issue at the link's `template`. */
function templateIssues(pkg: Record<string, unknown>, s: Record<string, unknown>, at: string, kind: unknown, def: unknown, props: Record<string, unknown>, refs: Record<string, unknown>, isFile: (field: unknown) => boolean): PackageIssue[] {
  const here = `${at}/template`
  if (s.mode !== `view`) return [{ path: here, message: `only a view link has a template` }]
  const templates = isObject(pkg.templates) ? pkg.templates : {}
  if (typeof s.template !== `string` || !own(templates, s.template)) return [{ path: here, message: `${String(s.template)} names no package template` }]
  const name = s.template
  const template = templates[name]
  if (!isObject(template) || !isObject(def) || typeof kind !== `string`) return []
  const issues: PackageIssue[] = []
  const say = (message: string) => issues.push({ path: here, message: `template ${name} ${message}` })
  const foreign = (source: string) => say(`reads ${source}: a public link reads only its own records:${kind}`)
  for (const b of Array.isArray(template.bindings) ? template.bindings : []) if (isObject(b) && typeof b.source === `string` && !readsSnapshot(b.source, kind, s.query)) foreign(b.source)
  const extensions = (Array.isArray(pkg.extensions) ? pkg.extensions : []).flatMap((e) => (isObject(e) && Array.isArray(e.components) ? (e.components as unknown[]) : []))
  for (const c of Array.isArray(template.components) ? template.components : []) {
    if (!isObject(c)) continue
    if (c.component === `RecordForm`) say(`holds a RecordForm: a view link only reads`)
    if (c.component === `RecordTable`) {
      const query = typeof c.query === `string` && c.query.trim() ? c.query.trim().replace(/^\?/, ``) : ``
      const source = `records:${String(c.kind)}${query ? `?${query}` : ``}`
      if (!readsSnapshot(source, kind, s.query)) foreign(source)
      if (c.editable !== false) say(`has an editable RecordTable: set editable: false`)
    }
    if (extensions.includes(c.component)) say(`uses the extension component ${String(c.component)}: a guest's page has no extensions`)
    for (const handler of isObject(c.on) ? Object.values(c.on) : []) {
      const call = isObject(handler) && isObject(handler.functionCall) ? handler.functionCall[`@call`] : undefined
      if (typeof call === `string` && !GUEST_CALLS.includes(call)) say(`calls ${call}: a guest's page only reads`)
    }
    const exp = expSource(c)
    if (exp !== null) say(`reads ${exp}: a guest has no exp: source`)
  }
  const exp = expSource(template.data)
  if (exp !== null) say(`reads ${exp}: a guest has no exp: source`)
  // What a guest's host will check: the expressions over the narrowed row and no query.
  const listed = Array.isArray(s.fields) && s.fields.every((f) => typeof f === `string`) ? (s.fields as string[]) : Object.keys(props).filter((f) => !isFile(props[f]) && !own(refs, f))
  const properties = Object.fromEntries(listed.filter((f) => own(props, f)).map((f) => [f, props[f]]))
  const schema = isObject(def.schema) ? def.schema : {}
  for (const issue of validatePackageExpressions({ templates: { [name]: guestTemplate(template, kind) }, records: { [kind]: { schema: { ...schema, properties } } } })) say(`on a guest's page: ${issue.message}`)
  return issues
}

/** Every problem with a package's `share`, each at its entry. */
export function validatePackageShares(pkg: Record<string, unknown>): PackageIssue[] {
  if (pkg.share === undefined) return []
  if (!Array.isArray(pkg.share)) return [{ path: `/share`, message: `share is a list of {name, mode, kind, fields?, query?, template?, expires?}` }]
  const issues: PackageIssue[] = []
  if (pkg.share.length > MAX_SHARES) issues.push({ path: `/share`, message: `at most ${MAX_SHARES} public links, got ${pkg.share.length}` })
  const records = isObject(pkg.records) ? pkg.records : {}
  const queries = isObject(pkg.queries) ? pkg.queries : {}
  const seen = new Set<string>()
  pkg.share.forEach((s, i) => {
    const at = `/share/${i}`
    if (!isObject(s)) return void issues.push({ path: at, message: `a public link is {name, mode, kind, fields?, query?, template?, expires?}` })
    for (const k of Object.keys(s).sort()) if (!SHARE_KEYS.includes(k)) issues.push({ path: `${at}/${k}`, message: `unknown public link key ${k}` })
    if (typeof s.name !== `string` || !SHARE_NAME_PATTERN.test(s.name)) issues.push({ path: `${at}/name`, message: `a name is a lowercase letter, then lowercase letters, digits and underscores (24 at most)` })
    else if (seen.has(s.name)) issues.push({ path: `${at}/name`, message: `duplicate public link ${s.name}` })
    else seen.add(s.name)
    if (typeof s.mode !== `string` || !SHARE_MODES.includes(s.mode)) issues.push({ path: `${at}/mode`, message: `mode is submit or view` })
    const def = typeof s.kind === `string` && own(records, s.kind) ? records[s.kind] : undefined
    if (!isObject(def)) issues.push({ path: `${at}/kind`, message: `${String(s.kind)} names no declared kind` })
    const schema = isObject(def) && isObject(def.schema) ? def.schema : {}
    const props = isObject(schema.properties) ? schema.properties : {}
    const required = Array.isArray(schema.required) ? (schema.required as unknown[]) : []
    const refs = isObject(def) && isObject(def.refs) ? def.refs : {}
    const isFile = (field: unknown) => isObject(field) && (field.format === `attachment` || (isObject(field.items) && field.items.format === `attachment`))
    if (s.fields !== undefined) {
      if (!Array.isArray(s.fields) || !s.fields.length || s.fields.some((f) => typeof f !== `string`)) issues.push({ path: `${at}/fields`, message: `fields is a non-empty list of field names` })
      else {
        s.fields.forEach((f: string, j: number) => {
          if (isObject(def) && !own(props, f)) issues.push({ path: `${at}/fields/${j}`, message: `${f} is not a field of ${String(s.kind)}` })
          const field = own(props, f) ? props[f] : undefined
          if (isFile(field)) issues.push({ path: `${at}/fields/${j}`, message: `a public link cannot carry the file field ${f}` })
          if (own(refs, f)) issues.push({ path: `${at}/fields/${j}`, message: `a public link cannot carry the ref field ${f}` })
        })
        if (s.mode === `submit`) for (const name of required) if (!s.fields.includes(name)) issues.push({ path: `${at}/fields`, message: `a submit link needs the required field ${String(name)}` })
      }
    }
    if (s.mode === `submit` && required.some((n) => typeof n === `string` && own(props, n) && isFile(props[n]))) issues.push({ path: `${at}/kind`, message: `a submit link cannot fill a required file field` })
    // A guest has no picker for a ref; with `fields` the checks above already speak.
    if (s.mode === `submit` && s.fields === undefined) for (const n of required) if (typeof n === `string` && own(refs, n)) issues.push({ path: `${at}/kind`, message: `a submit link cannot fill the required ref field ${n}` })
    if (s.query !== undefined) {
      if (s.mode !== `view`) issues.push({ path: `${at}/query`, message: `only a view link has a query` })
      else if (typeof s.query !== `string` || !own(queries, s.query)) issues.push({ path: `${at}/query`, message: `${String(s.query)} names no package query` })
      else {
        const q = queries[s.query]
        const from = isObject(q) && typeof q.from === `string` ? q.from.trim() : ``
        if (from && from !== s.kind) issues.push({ path: `${at}/query`, message: `query ${s.query} reads records:${from}, the link's kind is ${String(s.kind)}` })
      }
    }
    if (s.template !== undefined) issues.push(...templateIssues(pkg, s, at, s.kind, def, props, refs, isFile))
    if (s.expires !== undefined && (typeof s.expires !== `number` || !Number.isInteger(s.expires) || s.expires < 1 || s.expires > MAX_SHARE_EXPIRES_DAYS)) issues.push({ path: `${at}/expires`, message: `expires is a whole number of days, 1 to ${MAX_SHARE_EXPIRES_DAYS}` })
  })
  return issues
}
