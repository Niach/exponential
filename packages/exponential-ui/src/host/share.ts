// VAPP-241 (eApps P1, docs/package-format.md "Public links",
// fixtures/package-share.json): a package's PUBLIC LINK definitions. Only the
// declaration lives here; the links themselves (tokens, caps, the guest pages)
// are the host's. Pure; the Rust core mirrors it (host::share).

import type { PackageIssue } from "./package"

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
  /** Days from enabling until the link stops; default never. */
  expires?: number
}

const SHARE_KEYS = [`name`, `mode`, `kind`, `fields`, `query`, `expires`]
const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)

/** Every problem with a package's `share`, each at its entry. */
export function validatePackageShares(pkg: Record<string, unknown>): PackageIssue[] {
  if (pkg.share === undefined) return []
  if (!Array.isArray(pkg.share)) return [{ path: `/share`, message: `share is a list of {name, mode, kind, fields?, query?, expires?}` }]
  const issues: PackageIssue[] = []
  if (pkg.share.length > MAX_SHARES) issues.push({ path: `/share`, message: `at most ${MAX_SHARES} public links, got ${pkg.share.length}` })
  const records = isObject(pkg.records) ? pkg.records : {}
  const queries = isObject(pkg.queries) ? pkg.queries : {}
  const seen = new Set<string>()
  pkg.share.forEach((s, i) => {
    const at = `/share/${i}`
    if (!isObject(s)) return void issues.push({ path: at, message: `a public link is {name, mode, kind, fields?, query?, expires?}` })
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
    if (s.expires !== undefined && (typeof s.expires !== `number` || !Number.isInteger(s.expires) || s.expires < 1 || s.expires > MAX_SHARE_EXPIRES_DAYS)) issues.push({ path: `${at}/expires`, message: `expires is a whole number of days, 1 to ${MAX_SHARE_EXPIRES_DAYS}` })
  })
  return issues
}
