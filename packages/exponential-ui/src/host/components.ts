// VAPP-203: install checks every template the way the renderer will. Each
// template's components go through the reducer (`reduceSurface`: unknown
// components, missing or wrong props, child ids that name nothing, cycles,
// slots, styles), so a package that would render broken never installs.
// A reducer issue (keyed by a component id) becomes a JSON pointer: the
// component's own entry, or, for an id no component has, every place that
// names it (`children/<j>`, `children/componentId`, `slots/<name>`). The
// Rust core's `host::components` mirrors it (fixtures/host-router.json).

import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID } from "../catalog"
import { reduceSurface } from "../reducer"
import type { ExtensionDef, FlatComponent } from "../types"

export interface ComponentIssue {
  path: string
  message: string
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const escape = (key: string) => key.replace(/~/g, `~0`).replace(/\//g, `~1`)
const BUILT_IN = [CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, A2UI_BASIC_CATALOG_ID]

/** The shape the reducer reads (the Rust core parses it): `children` = ids
 *  or `{componentId, path, key?}`, `slots` = ids by name, `on` and `style`
 *  objects. Null counts as unset. */
function shapeIssues(c: Record<string, unknown>, at: string): ComponentIssue[] {
  const out: ComponentIssue[] = []
  const ch = c.children
  if (ch !== undefined && ch !== null) {
    const ids = Array.isArray(ch) && ch.every((x) => typeof x === `string`)
    const template = isObject(ch) && typeof ch.componentId === `string` && typeof ch.path === `string` && (ch.key === undefined || ch.key === null || typeof ch.key === `string`)
    if (!ids && !template) out.push({ path: `${at}/children`, message: `children is a list of ids or {componentId, path, key?}` })
  }
  if (c.slots !== undefined && c.slots !== null && (!isObject(c.slots) || Object.values(c.slots).some((x) => typeof x !== `string`)))
    out.push({ path: `${at}/slots`, message: `slots is an object of component ids` })
  if (c.on !== undefined && c.on !== null && !isObject(c.on)) out.push({ path: `${at}/on`, message: `on is an object of actions` })
  if (c.style !== undefined && c.style !== null && !isObject(c.style)) out.push({ path: `${at}/style`, message: `style is an object` })
  return out
}

/** Every place a component list names `id` as a child. */
function references(components: Record<string, unknown>[], id: string, at: string): string[] {
  const out: string[] = []
  components.forEach((c, i) => {
    const here = `${at}/${i}`
    if (Array.isArray(c.children)) c.children.forEach((x, j) => x === id && out.push(`${here}/children/${j}`))
    else if (isObject(c.children) && c.children.componentId === id) out.push(`${here}/children/componentId`)
    if (isObject(c.slots)) for (const [name, x] of Object.entries(c.slots)) if (x === id) out.push(`${here}/slots/${escape(name)}`)
  })
  return out
}

/** VAPP-203: every template's components, reduced as the host will render
 *  them. `catalogIds` = the host's (an unsupported catalog is reported
 *  elsewhere); `defs` = the extension definitions the host knows. A catalog
 *  or declared extension the host has no definition for leaves its
 *  component names open (an unknown component is not reported), every
 *  other check still holds. */
export function validatePackageComponents(pkg: Record<string, unknown>, catalogIds: readonly string[], defs: readonly ExtensionDef[] = []): ComponentIssue[] {
  const issues: ComponentIssue[] = []
  const catalogId = typeof pkg.catalogId === `string` ? pkg.catalogId : ``
  if (!catalogIds.includes(catalogId) || !isObject(pkg.templates)) return issues
  const known = (id: string) => BUILT_IN.includes(id) || defs.some((d) => d.id === id)
  const declared = Array.isArray(pkg.extensions) ? pkg.extensions.map((e) => (isObject(e) && typeof e.id === `string` ? e.id : ``)) : []
  const open = !known(catalogId) || declared.some((id) => id && !known(id))
  const reduceAs = known(catalogId) ? catalogId : CORE_CATALOG_ID
  for (const [id, template] of Object.entries(pkg.templates)) {
    if (!isObject(template) || !Array.isArray(template.components)) continue
    const at = `/templates/${escape(id)}/components`
    const components = template.components
    // The structure is validatePackage's own issue; reduce only a clean list.
    const ids = new Set<string>()
    let clean = components.length > 0
    for (const c of components) {
      if (!isObject(c) || typeof c.id !== `string` || typeof c.component !== `string` || ids.has(c.id)) clean = false
      else ids.add(c.id)
    }
    if (!clean || !ids.has(`root`)) continue
    const list = components as Record<string, unknown>[]
    const shape = list.flatMap((c, i) => shapeIssues(c, `${at}/${i}`))
    if (shape.length) {
      issues.push(...shape)
      continue
    }
    const index = new Map(list.map((c, i) => [c.id as string, i]))
    const seen = new Set<string>()
    const push = (path: string, message: string) => {
      const key = `${path}\n${message}`
      if (seen.has(key)) return
      seen.add(key)
      issues.push({ path, message })
    }
    for (const issue of reduceSurface(list as FlatComponent[], { catalogId: reduceAs, extensions: defs }).issues) {
      if (open && issue.message.startsWith(`unknown component `)) continue
      const i = index.get(issue.id)
      if (i !== undefined) {
        push(`${at}/${i}`, issue.message)
        continue
      }
      const refs = references(list, issue.id, at)
      if (refs.length === 0) push(at, `${issue.id}: ${issue.message}`)
      for (const ref of refs) push(ref, `${issue.id}: ${issue.message}`)
    }
  }
  return issues
}
