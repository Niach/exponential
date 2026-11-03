// VAPP-91: catalog negotiation and declarative app packages (VAPP-82: a
// manifest + A2UI templates + bindings, data never code).

import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID } from "../catalog"
import { A2UI_VERSION } from "../catalog"
import type { ExtensionDef, FlatComponent } from "../types"
import type { ServerMessage } from "./contract"
import { matchesPattern } from "./policy"
import { parseSource } from "./sources"
import { RECORD_KIND_PATTERN, validateRecordRefs, validateRecordSchema } from "./records"
import { validateRecordPolicy } from "./record-policy"
import type { RecordKind } from "./records"
import { RECORDS_SCHEME, parseRecordsJson } from "./query"
import { validateTools } from "./tools"
import type { PackageTool } from "./tools"
import { validatePackageExposes, validateUpstreams } from "./exposes"
import type { PackageExpose } from "./exposes"
import { recordRowType, validatePackageExpressions, validateRecordComputed } from "./expressions"
import { requiredType, type ExprType } from "../cel"
import { validatePackageActions, validatePackageEffects } from "./actions"
import { packageTestIssues, validatePackageTests } from "./tests"
import { validatePackageComponents } from "./components"

/** VAPP-161: the package format version this host reads
 *  (catalog/host.json `package.formatVersion`; docs/package-format.md).
 *  A package names it, and it is exactly this one: no older format is
 *  read (eApps keeps no backwards compatibility). */
export const PACKAGE_FORMAT_VERSION = 3

/** The core, the core lite, the A2UI basic catalog, then every registered
 *  extension id in order, de-duplicated. */
export function supportedCatalogIds(extensionIds: readonly string[] = []): string[] {
  const out: string[] = []
  for (const id of [CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, A2UI_BASIC_CATALOG_ID, ...extensionIds]) if (!out.includes(id)) out.push(id)
  return out
}

/** A2UI's `a2uiClientCapabilities`. */
export function clientCapabilities(extensionIds: readonly string[] = []): { "v0.9": { supportedCatalogIds: string[] } } {
  return { "v0.9": { supportedCatalogIds: supportedCatalogIds(extensionIds) } }
}

export interface TemplateBinding {
  path: string
  source: string
}

export interface AppTemplate {
  components: FlatComponent[]
  data?: unknown
  bindings?: TemplateBinding[]
}

export interface AppPackage {
  id: string
  name: string
  version: string
  catalogId: string
  templates: Record<string, AppTemplate>
  /** Function patterns the package's surfaces may call (allowlist). */
  functions?: string[]
  /** VAPP-107: the binding sources the package's surfaces may follow
   *  (`exp:devices`, or a prefix `exp:*`); unset = any registered scheme.
   *  Set, a template binding or a later `bindDataModel` outside it is
   *  refused (`sourceAllowed`). */
  sources?: string[]
  /** A built-in theme id or a theme JSON. */
  theme?: unknown
  icon?: { glyph?: string; color?: string }
  description?: string
  /** VAPP-122: the app's own data, one JSON Schema subset per kind
   *  (`host/records.ts`); a record is validated against its kind before
   *  it is stored. A kind's name follows `RECORD_KIND_PATTERN`. */
  records?: Record<string, RecordKind>
  /** VAPP-161: the format version the package was written for
   *  (`PACKAGE_FORMAT_VERSION`, required). */
  formatVersion: number
  /** VAPP-161: MCP servers and scripts its functions call (`tools.<id>…`),
   *  each behind the host's consent card (`host/tools.ts`). */
  tools?: Record<string, PackageTool>
  /** VAPP-161: the extension catalogs it needs beyond `catalogId`. */
  extensions?: PackageExtension[]
  /** VAPP-180: the MCP tools it exposes to a host's MCP broker
   *  (`host/exposes.ts`): declarative bindings over its records, the host's
   *  actions or its upstreams, run as the calling person. */
  exposes?: PackageExpose[]
  /** VAPP-180: the MCP server names (the host's, e.g. a team's servers) its
   *  `upstream` bindings call. */
  upstreams?: string[]
  /** VAPP-190: named JSON records queries (`parseRecordsJson`); a binding
   *  `records:<kind>?query=<name>&<params>` follows one, its `${param}`
   *  templates filled from the binding's other params. */
  queries?: Record<string, unknown>
  /** VAPP-194: named actions, the only place a package writes
   *  (`host/actions.ts`): `{params?, steps}`, run by `actions.<name>`. */
  actions?: Record<string, unknown>
  /** VAPP-196: the package's tests (`host/tests.ts`), run at install. */
  tests?: unknown[]
}

/** A package query's name. */
export const QUERY_NAME_PATTERN = /^[a-z][A-Za-z0-9_]{0,31}$/

/** VAPP-161: an extension catalog a package needs: its id, the minimum
 *  `MAJOR.MINOR.PATCH` (caret) and the components it uses from it. */
export interface PackageExtension {
  id: string
  version: string
  components?: string[]
}

/** An extension as a host registered it (`availableExtensions`). */
export interface AvailableExtension {
  id: string
  /** Unset = satisfies any version. */
  version?: string
  /** Unset = not checked. */
  components?: readonly string[]
  /** VAPP-203: its definition, so install checks its components' props
   *  (unset = its component names stay open). */
  def?: ExtensionDef
}

/** A host's extension catalogs as `validatePackage` checks them. */
export function availableExtensions(defs: readonly ExtensionDef[]): AvailableExtension[] {
  return defs.map((d) => ({ id: d.id, ...(d.version ? { version: d.version } : {}), components: [...Object.keys(d.components), ...Object.keys(d.macros ?? {})], def: d }))
}

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/

/** Caret: `have` has `want`'s major and is not lower. */
export function versionSatisfies(have: string, want: string): boolean {
  const h = SEMVER.exec(have)
  const w = SEMVER.exec(want)
  if (!h || !w) return false
  const [hM, hm, hp] = [Number(h[1]), Number(h[2]), Number(h[3])]
  const [wM, wm, wp] = [Number(w[1]), Number(w[2]), Number(w[3])]
  return hM === wM && (hm > wm || (hm === wm && hp >= wp))
}

export interface PackageIssue {
  path: string
  message: string
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const POINTER = /^(\/([^/~]|~[01])*)*$/

/** Every problem with a package, each with a JSON pointer into it.
 *  `extensions` = the host's registered extensions (versions and
 *  components); unset, a declared extension only has to be in `catalogIds`. */
export function validatePackage(pkg: unknown, catalogIds: readonly string[] = supportedCatalogIds(), extensions?: readonly AvailableExtension[]): PackageIssue[] {
  const issues: PackageIssue[] = []
  if (!isObject(pkg)) return [{ path: ``, message: `a package is an object` }]
  if (pkg.formatVersion === undefined) issues.push({ path: `/formatVersion`, message: `formatVersion is required (${PACKAGE_FORMAT_VERSION})` })
  else if (typeof pkg.formatVersion !== `number` || !Number.isInteger(pkg.formatVersion) || pkg.formatVersion < 1) issues.push({ path: `/formatVersion`, message: `formatVersion is a positive integer` })
  else if (pkg.formatVersion > PACKAGE_FORMAT_VERSION) issues.push({ path: `/formatVersion`, message: `format version ${pkg.formatVersion} is newer than this host (${PACKAGE_FORMAT_VERSION})` })
  else if (pkg.formatVersion < PACKAGE_FORMAT_VERSION) issues.push({ path: `/formatVersion`, message: `format version ${pkg.formatVersion} is no longer read: write ${PACKAGE_FORMAT_VERSION}` })
  for (const key of [`id`, `name`, `version`, `catalogId`])
    if (typeof pkg[key] !== `string` || !(pkg[key] as string)) issues.push({ path: `/${key}`, message: `${key} is a required string` })
  if (typeof pkg.catalogId === `string` && pkg.catalogId && !catalogIds.includes(pkg.catalogId))
    issues.push({ path: `/catalogId`, message: `unsupported catalog ${pkg.catalogId}` })
  if (!isObject(pkg.templates) || !Object.keys(pkg.templates).length) issues.push({ path: `/templates`, message: `templates is a non-empty object` })
  else
    for (const [id, template] of Object.entries(pkg.templates)) {
      const at = `/templates/${id}`
      if (!isObject(template)) {
        issues.push({ path: at, message: `a template is an object` })
        continue
      }
      const components = template.components
      if (!Array.isArray(components) || !components.length) issues.push({ path: `${at}/components`, message: `components is a non-empty array` })
      else {
        const ids = new Set<string>()
        components.forEach((c, i) => {
          if (!isObject(c) || typeof c.id !== `string` || typeof c.component !== `string`) issues.push({ path: `${at}/components/${i}`, message: `a component needs an id and a component` })
          else if (ids.has(c.id)) issues.push({ path: `${at}/components/${i}/id`, message: `duplicate id ${c.id}` })
          else ids.add(c.id)
        })
        if (!ids.has(`root`)) issues.push({ path: `${at}/components`, message: `no component has the id root` })
      }
      if (template.bindings !== undefined) {
        if (!Array.isArray(template.bindings)) issues.push({ path: `${at}/bindings`, message: `bindings is an array` })
        else
          template.bindings.forEach((b, i) => {
            const bp = `${at}/bindings/${i}`
            if (!isObject(b) || typeof b.path !== `string` || !POINTER.test(b.path)) issues.push({ path: `${bp}/path`, message: `path is a JSON pointer` })
            if (!isObject(b) || typeof b.source !== `string` || !parseSource(b.source)) issues.push({ path: `${bp}/source`, message: `source is a scheme:name URI` })
          })
      }
    }
  if (pkg.functions !== undefined && (!Array.isArray(pkg.functions) || pkg.functions.some((f) => typeof f !== `string` || !f)))
    issues.push({ path: `/functions`, message: `functions is a list of names or prefix* patterns` })
  if (pkg.records !== undefined) {
    if (!isObject(pkg.records)) issues.push({ path: `/records`, message: `records is an object of kinds` })
    else
      for (const kind of Object.keys(pkg.records).sort()) {
        const def = (pkg.records as Record<string, unknown>)[kind]
        const at = `/records/${kind}`
        if (!RECORD_KIND_PATTERN.test(kind)) issues.push({ path: at, message: `a kind is lowercase letters, digits and underscores` })
        if (!isObject(def)) issues.push({ path: at, message: `a kind is {schema, title?, policy?}` })
        else {
          issues.push(...validateRecordSchema(def.schema, `${at}/schema`))
          // eApps R3: who may read, create, update and delete (record-policy.json).
          if (def.policy !== undefined) issues.push(...validateRecordPolicy(def.policy, `${at}/policy`))
          // VAPP-214 (Q3): computed fields, typed against the schema.
          issues.push(...validateRecordComputed(def, at))
          // eApps R5 (VAPP-219): refs name a string field and a declared kind.
          issues.push(...validateRecordRefs(def, at, pkg.records))
        }
      }
  }
  if (pkg.sources !== undefined && (!Array.isArray(pkg.sources) || pkg.sources.some((f) => typeof f !== `string` || !f)))
    issues.push({ path: `/sources`, message: `sources is a list of scheme:name sources or prefix* patterns` })
  else if (Array.isArray(pkg.sources) && isObject(pkg.templates))
    for (const [id, template] of Object.entries(pkg.templates)) {
      if (!isObject(template) || !Array.isArray(template.bindings)) continue
      template.bindings.forEach((b, i) => {
        if (isObject(b) && typeof b.source === `string` && parseSource(b.source) && !sourceAllowed(pkg.sources as string[], b.source))
          issues.push({ path: `/templates/${id}/bindings/${i}/source`, message: `source ${b.source} is not in the package's sources` })
      })
    }
  // VAPP-161: a `records:<kind>` binding names a declared kind.
  if (isObject(pkg.templates))
    for (const [id, template] of Object.entries(pkg.templates)) {
      if (!isObject(template) || !Array.isArray(template.bindings)) continue
      template.bindings.forEach((b, i) => {
        const parsed = isObject(b) && typeof b.source === `string` ? parseSource(b.source) : null
        if (parsed?.scheme !== RECORDS_SCHEME) return
        if (!isObject(pkg.records) || !Object.prototype.hasOwnProperty.call(pkg.records, parsed.name))
          issues.push({ path: `/templates/${id}/bindings/${i}/source`, message: `records:${parsed.name} names no declared kind` })
      })
    }
  // VAPP-190: the named queries parse clean, read a declared kind, and a
  // records binding's `query=` names one of them.
  if (pkg.queries !== undefined) {
    if (!isObject(pkg.queries)) issues.push({ path: `/queries`, message: `queries is an object of named queries` })
    else
      for (const name of Object.keys(pkg.queries).sort()) {
        const at = `/queries/${name}`
        if (!QUERY_NAME_PATTERN.test(name)) issues.push({ path: at, message: `a query name is a letter, then letters, digits and underscores` })
        const q = (pkg.queries as Record<string, unknown>)[name]
        for (const issue of parseRecordsJson(q).issues) issues.push({ path: `${at}${issue.path}`, message: issue.message })
        const from = isObject(q) && typeof q.from === `string` ? q.from.trim() : ``
        if (from && !from.includes(`\${`) && (!isObject(pkg.records) || !Object.prototype.hasOwnProperty.call(pkg.records, from)))
          issues.push({ path: `${at}/from`, message: `records:${from} names no declared kind` })
        else if (from && !from.includes(`\${`) && isObject(q)) issues.push(...queryFieldIssues(q, from, (pkg.records as Record<string, unknown>)[from], at))
        // eApps R5: an expanded field is a ref of the kind.
        if (from && isObject(q) && Array.isArray(q.expand) && isObject(pkg.records) && isObject((pkg.records as Record<string, unknown>)[from]))
          q.expand.forEach((f, i) => {
            const refs = ((pkg.records as Record<string, Record<string, unknown>>)[from]!.refs ?? {}) as Record<string, unknown>
            if (typeof f === `string` && f.trim() && !(isObject(refs) && Object.prototype.hasOwnProperty.call(refs, f.trim()))) issues.push({ path: `${at}/expand/${i}`, message: `${f.trim()} is not a ref of ${from}` })
          })
      }
  }
  // Dogfood G: an exposed records run's `query` names a package query of the run's kind.
  if (Array.isArray(pkg.exposes))
    pkg.exposes.forEach((expose, i) => {
      const run = isObject(expose) && isObject(expose.run) && isObject(expose.run.records) ? expose.run.records : null
      if (!run || typeof run.query !== `string`) return
      const at = `/exposes/${i}/run/records/query`
      const q = isObject(pkg.queries) && Object.prototype.hasOwnProperty.call(pkg.queries, run.query) ? (pkg.queries as Record<string, unknown>)[run.query] : undefined
      if (q === undefined) return void issues.push({ path: at, message: `query ${run.query} names no package query` })
      const from = isObject(q) && typeof q.from === `string` ? q.from.trim() : ``
      if (from && !from.includes(`\${`) && typeof run.kind === `string` && from !== run.kind)
        issues.push({ path: at, message: `query ${run.query} reads records:${from}, the tool's kind is ${run.kind}` })
    })
  if (isObject(pkg.templates))
    for (const [id, template] of Object.entries(pkg.templates)) {
      if (!isObject(template) || !Array.isArray(template.bindings)) continue
      template.bindings.forEach((b, i) => {
        const parsed = isObject(b) && typeof b.source === `string` ? parseSource(b.source) : null
        const name = parsed?.params.query
        if (!parsed || name === undefined || parsed.scheme !== RECORDS_SCHEME) return
        if (!isObject(pkg.queries) || !Object.prototype.hasOwnProperty.call(pkg.queries, name))
          issues.push({ path: `/templates/${id}/bindings/${i}/source`, message: `query ${name} names no package query` })
      })
    }
  // VAPP-203: every template reduces clean, as the host will render it.
  issues.push(...validatePackageComponents(pkg, catalogIds, (extensions ?? []).flatMap((e) => (e.def ? [e.def] : []))))
  // VAPP-191: every expression in a template parses and type-checks.
  issues.push(...validatePackageExpressions(pkg))
  // VAPP-194: bindings are pure; actions are checked step by step.
  issues.push(...validatePackageEffects(pkg))
  issues.push(...validatePackageActions(pkg))
  if (pkg.tools !== undefined) issues.push(...validateTools(pkg.tools))
  if (pkg.upstreams !== undefined) issues.push(...validateUpstreams(pkg.upstreams))
  if (pkg.exposes !== undefined) issues.push(...validatePackageExposes(pkg.exposes, pkg.records, pkg.upstreams))
  if (pkg.extensions !== undefined) issues.push(...validateExtensions(pkg.extensions, catalogIds, extensions))
  // VAPP-196: the package's tests: their shape, then (on an otherwise valid
  // package) every one run; a failing test is an issue like a type error.
  issues.push(...validatePackageTests(pkg))
  if (issues.length === 0) issues.push(...packageTestIssues(pkg))
  return issues
}

/** Dogfood G: a named query reads fields its kind has. Every field it filters,
 *  groups, aggregates, sums, sorts, buckets or runs on is a schema field, a
 *  computed field or a row key (a sort, a running value: or one of its
 *  aggregates, `count`), and `sum`/`avg` read a number. Else the query
 *  silently reads null. Issues at the query's top-level key. */
function queryFieldIssues(json: Record<string, unknown>, kind: string, def: unknown, at: string): PackageIssue[] {
  const { query } = parseRecordsJson(json, kind)
  const row = recordRowType(def) as Record<string, ExprType>
  const has = (f: string) => Object.prototype.hasOwnProperty.call(row, f)
  const outputs = new Set([`count`, ...query.aggregate.map((a) => a.name), ...query.group.map((g) => g.field)])
  const issues: PackageIssue[] = []
  const seen = new Set<string>()
  const unknown = (key: string, f: string, also?: boolean) => {
    if (has(f) || also || seen.has(`${key}\u0000${f}`)) return
    seen.add(`${key}\u0000${f}`)
    issues.push({ path: `${at}/${key}`, message: `${f} is no field of records:${kind}` })
  }
  const numeric = (key: string, op: string, f: string) => {
    const t = has(f) ? requiredType(row[f]!) : `dyn`
    if (t !== `number` && t !== `dyn`) issues.push({ path: `${at}/${key}`, message: `${op} reads a number: ${f} is ${typeof t === `string` ? t : Array.isArray(t) ? `list` : `map`}` })
  }
  for (const f of query.where) unknown(`where`, f.field)
  for (const alt of query.or) for (const f of alt) unknown(`or`, f.field)
  query.group.forEach((g, i) => unknown(Array.isArray(json.group) ? `group/${i}` : `group`, g.field))
  for (const a of query.aggregate) {
    if (a.field === null) continue
    unknown(`aggregate/${a.name}`, a.field)
    if ((a.op === `sum` || a.op === `avg`) && has(a.field)) numeric(`aggregate/${a.name}`, a.op, a.field)
  }
  if (query.sum !== null) {
    unknown(`sum`, query.sum)
    if (has(query.sum)) numeric(`sum`, `sum`, query.sum)
  }
  if (query.month) unknown(`month`, query.month.field)
  if (query.before) unknown(`before`, query.before.field)
  query.sort.forEach((s, i) => unknown(Array.isArray(json.sort) ? `sort/${i}` : `sort`, s.field, outputs.has(s.field)))
  for (const r of query.running) unknown(`running/${r.field}`, r.field, outputs.has(r.field))
  return issues
}

function validateExtensions(list: unknown, catalogIds: readonly string[], available: readonly AvailableExtension[] | undefined): PackageIssue[] {
  if (!Array.isArray(list)) return [{ path: `/extensions`, message: `extensions is a list of {id, version, components?}` }]
  const issues: PackageIssue[] = []
  const seen = new Set<string>()
  list.forEach((ext, i) => {
    const at = `/extensions/${i}`
    if (!isObject(ext)) return void issues.push({ path: at, message: `an extension is {id, version, components?}` })
    const id = typeof ext.id === `string` && ext.id ? ext.id : null
    if (!id) issues.push({ path: `${at}/id`, message: `id is a required string` })
    const version = typeof ext.version === `string` && SEMVER.test(ext.version) ? ext.version : null
    if (!version) issues.push({ path: `${at}/version`, message: `version is MAJOR.MINOR.PATCH` })
    const components = ext.components === undefined ? [] : Array.isArray(ext.components) && ext.components.every((c) => typeof c === `string` && c) ? (ext.components as string[]) : null
    if (!components) issues.push({ path: `${at}/components`, message: `components is a list of component names` })
    if (!id) return
    if (seen.has(id)) return void issues.push({ path: `${at}/id`, message: `duplicate extension ${id}` })
    seen.add(id)
    const have = available?.find((a) => a.id === id)
    if (!catalogIds.includes(id) || (available && !have)) return void issues.push({ path: `${at}/id`, message: `extension ${id} is not available on this host` })
    if (!have) return
    if (version && have.version !== undefined && !versionSatisfies(have.version, version)) issues.push({ path: `${at}/version`, message: `extension ${id} ${have.version} does not satisfy ^${version}` })
    if (have.components)
      (components ?? []).forEach((name, j) => {
        if (!have.components!.includes(name)) issues.push({ path: `${at}/components/${j}`, message: `extension ${id} has no component ${name}` })
      })
  })
  return issues
}

/** VAPP-107: may a package whose `sources` list is `allowed` follow this
 *  source URI? Unset = yes; else the URI's `scheme:name` (scheme
 *  lower-cased, the query dropped) must match a listed source exactly or a
 *  `prefix*` pattern. */
export function sourceAllowed(allowed: readonly string[] | undefined, source: string): boolean {
  if (allowed === undefined) return true
  const parsed = parseSource(source)
  if (!parsed) return false
  const name = `${parsed.scheme}:${parsed.name}`
  return allowed.some((p) => matchesPattern(p, name) || matchesPattern(p, source))
}

/** A template as the messages that create it: createSurface, the
 *  components, the merged initial data, one bindDataModel per binding. */
export function templateMessages(pkg: AppPackage, templateId: string, surfaceId: string, data?: unknown): ServerMessage[] | null {
  const template = pkg.templates[templateId]
  if (!template) return null
  const version = A2UI_VERSION
  const out: ServerMessage[] = [
    { version, createSurface: { surfaceId, catalogId: pkg.catalogId } },
    { version, updateComponents: { surfaceId, components: template.components } },
  ]
  const merged = mergeData(template.data, data)
  if (merged !== undefined) out.push({ version, updateDataModel: { surfaceId, path: `/`, value: merged } })
  for (const b of template.bindings ?? []) out.push({ version, bindDataModel: { surfaceId, path: b.path, source: b.source } })
  return out
}

/** The template's data under the caller's (objects merge one level deep in
 *  key order; anything else: the caller's wins). */
export function mergeData(base: unknown, over: unknown): unknown {
  if (over === undefined) return base
  if (isObject(base) && isObject(over)) return { ...base, ...over }
  return over
}
