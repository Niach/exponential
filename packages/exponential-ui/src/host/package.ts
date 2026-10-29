// VAPP-91: catalog negotiation and declarative app packages (VAPP-82: a
// manifest + A2UI templates + bindings, data never code).

import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID } from "../catalog"
import { A2UI_VERSION } from "../catalog"
import type { FlatComponent } from "../types"
import type { ServerMessage } from "./contract"
import { matchesPattern } from "./policy"
import { parseSource } from "./sources"
import { RECORD_KIND_PATTERN, validateRecordSchema } from "./records"
import type { RecordKind } from "./records"
import { RECORDS_SCHEME } from "./record-store"
import { validateTools } from "./tools"
import type { PackageTool } from "./tools"

/** VAPP-161: the package format version this host reads
 *  (catalog/host.json `package.formatVersion`; docs/package-format.md).
 *  A package without `formatVersion` is version 1; every version-1
 *  package is a valid version-2 package. */
export const PACKAGE_FORMAT_VERSION = 2

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
  /** VAPP-161: the format version the package was written for (default 1). */
  formatVersion?: number
  /** VAPP-161: MCP servers and scripts its functions call (`tools.<id>…`),
   *  each behind the host's consent card (`host/tools.ts`). */
  tools?: Record<string, PackageTool>
  /** VAPP-161: the extension catalogs it needs beyond `catalogId`. */
  extensions?: PackageExtension[]
}

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
}

/** A host's extension catalogs as `validatePackage` checks them. */
export function availableExtensions(defs: readonly { id: string; version?: string; components: Record<string, unknown>; macros?: Record<string, unknown> }[]): AvailableExtension[] {
  return defs.map((d) => ({ id: d.id, ...(d.version ? { version: d.version } : {}), components: [...Object.keys(d.components), ...Object.keys(d.macros ?? {})] }))
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
  if (pkg.formatVersion !== undefined) {
    if (typeof pkg.formatVersion !== `number` || !Number.isInteger(pkg.formatVersion) || pkg.formatVersion < 1) issues.push({ path: `/formatVersion`, message: `formatVersion is a positive integer` })
    else if (pkg.formatVersion > PACKAGE_FORMAT_VERSION) issues.push({ path: `/formatVersion`, message: `format version ${pkg.formatVersion} is newer than this host (${PACKAGE_FORMAT_VERSION})` })
  }
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
        if (!isObject(def)) issues.push({ path: at, message: `a kind is {schema, title?}` })
        else issues.push(...validateRecordSchema(def.schema, `${at}/schema`))
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
  if (pkg.tools !== undefined) issues.push(...validateTools(pkg.tools))
  if (pkg.extensions !== undefined) issues.push(...validateExtensions(pkg.extensions, catalogIds, extensions))
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
