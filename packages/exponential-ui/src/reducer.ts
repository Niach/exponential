// VAPP-85: the TypeScript REFERENCE reducer. A surface's flat component list
// (A2UI `updateComponents`, core OR basic catalog) → ONE normalized tree in
// the core vocabulary, macros expanded, every unknown component the explicit
// `Unknown` placeholder. The Rust core (VAPP-86) replays the same fixtures.

import {
  A2UI_BASIC_CATALOG_ID,
  A2UI_VERSION,
  ACCEPT_V09,
  CORE_CATALOG_ID,
  CORE_LITE_CATALOG_ID,
  UNKNOWN_COMPONENT,
  coreCatalog,
  sharedCatalogView,
} from "./catalog"
import { SURFACE_COMPONENT, checkEntry, escapeToken } from "./entry"
import { upgradeCard09 } from "./upgrade09"
import { mapBasicComponent } from "./basic-map"
import { isDynamic } from "./expr"
import { expandMacros } from "./macros"
import { isKnownFunctionName, validateNode, validateProps } from "./validate"
import { LIMIT_ISSUES, MAX_COMPONENTS, MAX_DEPTH } from "./limits"
import type {
  ChildTemplate,
  ComponentDef,
  ExtensionDef,
  FlatComponent,
  NestedNode,
  ReduceIssue,
  UiNode,
  Visible,
  WireChildTemplate,
} from "./types"

/** Round 9 §5.3: the ENVELOPE keys (`ComponentCommon`); everything else is
 *  a declared prop of the resolved catalog component. */
export const RESERVED_KEYS: readonly string[] = [`id`, `component`, `catalogId`, `accessibility`, `metadata`]
/** The declared props the reducer lifts out of `props` into the node. */
const STRUCTURAL = new Set([...RESERVED_KEYS, `children`, `slots`, `on`, `style`, `visible`])
/** Round 9 §5.1: the closed `accessibility` object. */
export const ACCESSIBILITY_KEYS: readonly string[] = [`label`, `description`, `live`, `hidden`, `selected`, `expanded`]

/** #EXP-1629: the state keys (DynamicBoolean: a boolean or a binding). */
export const ACCESSIBILITY_STATES: readonly string[] = [`selected`, `expanded`]
const LIVE_VALUES: readonly string[] = [`off`, `polite`, `assertive`]

/** Round 9 §5.2: a reducer issue with its code and path
 *  (`/components/<id>` + the prop pointer). */
export function reduceIssue(id: string, message: string, pointer = ``, code = `VALIDATION_FAILED`): ReduceIssue {
  return { id, message, code, path: `/components/${escapeToken(id)}${pointer}` }
}

/** A validate.ts dotted path (`props.items[0].label`, `style.color`,
 *  `on.press.functionCall`) → the component pointer (`/items/0/label`). */
export function issuePointer(path: string): string {
  const tokens: string[] = []
  const rest = path.startsWith(`props.`) ? path.slice(6) : path === `props` ? `` : path
  for (const part of rest.split(`.`)) {
    if (part === ``) continue
    const m = /^([^[]*)((?:\[\d+\])*)$/.exec(part)
    if (!m) {
      tokens.push(part)
      continue
    }
    if (m[1]) tokens.push(m[1])
    for (const idx of m[2]!.matchAll(/\[(\d+)\]/g)) tokens.push(idx[1]!)
  }
  return tokens.map((t) => `/${escapeToken(t)}`).join(``)
}

export interface ReduceOptions {
  /** The surface's DEFAULT catalog (round 9 §5.5: a component's own
   *  `catalogId` wins): the core, the lite subset, the basic catalog or an
   *  extension's id; absent = none (a component without its own catalog is
   *  the Unknown placeholder). */
  catalogId?: string
  /** Round 9 §5.5: the router's AVAILABLE catalog ids (default: core,
   *  core-lite, basic, then the extensions' ids). */
  available?: readonly string[]
  extensions?: readonly ExtensionDef[]
  /** The root component id (A2UI: `root`). */
  rootId?: string
  /** Expand macros (default true); false keeps the pre-expansion tree. */
  expand?: boolean
  /** Record prop validation issues (default true). */
  validate?: boolean
}

export interface ReduceResult {
  root: UiNode
  issues: ReduceIssue[]
  /** Round 2 (docs/round-2-contract.md §4): the nodes a data `template`
   *  renders per item, by component id, LIFTED out of the tree (a template
   *  node listed as a child too never renders in place), validated and
   *  expanded like the root; in discovery order. Absent without templates. */
  templates?: Record<string, UiNode>
}

/** Lift every node a `template.component` names out of the trees (children
 *  and slots, at any depth, the root excepted) into the template table;
 *  ids no tree holds are built with `buildMissing` (the flat path), else
 *  reported. Nested templates (a template inside a template node) are
 *  lifted too. A template that would instantiate ITSELF (its own owner, an
 *  ancestor of its owner, or through other templates) and one naming the
 *  root are reported (`template: cycle through this id`) and never lifted:
 *  the node stays where it was and its owner renders no items. Mutates the
 *  freshly built trees. */
function liftTemplates(root: UiNode, issues: ReduceIssue[], buildMissing?: (id: string) => UiNode | undefined): Map<string, UiNode> {
  const order: string[] = []
  const want = new Set<string>()
  const found = new Map<string, UiNode>()
  const missing = new Set<string>()
  let rootNamed = false
  // Every node strip changed, as it was first (to put a cyclic template back).
  const originals = new Map<UiNode, { children: UiNode[]; slots?: Record<string, UiNode> }>()
  const collect = (n: UiNode) => {
    const id = n.template?.component
    if (id === root.id) rootNamed = true
    else if (id !== undefined && !want.has(id)) {
      want.add(id)
      order.push(id)
    }
    for (const slot of Object.values(n.slots ?? {})) collect(slot)
    n.children.forEach(collect)
  }
  const strip = (n: UiNode) => {
    const lifts = n.children.some((c) => want.has(c.id)) || Object.values(n.slots ?? {}).some((c) => want.has(c.id))
    if (lifts && !originals.has(n)) originals.set(n, { children: [...n.children], ...(n.slots ? { slots: { ...n.slots } } : {}) })
    n.children = n.children.filter((c) => {
      if (!want.has(c.id)) return true
      if (!found.has(c.id)) found.set(c.id, c)
      return false
    })
    if (n.slots) {
      for (const [name, slot] of Object.entries(n.slots)) {
        if (!want.has(slot.id)) continue
        if (!found.has(slot.id)) found.set(slot.id, slot)
        delete n.slots[name]
      }
      if (Object.keys(n.slots).length === 0) delete n.slots
    }
    for (const slot of Object.values(n.slots ?? {})) strip(slot)
    n.children.forEach(strip)
  }
  collect(root)
  for (let before = ``; before !== `${want.size}/${found.size}/${missing.size}`; ) {
    before = `${want.size}/${found.size}/${missing.size}`
    strip(root)
    for (let n = -1; n !== found.size; ) {
      n = found.size
      for (const node of [...found.values()]) strip(node)
    }
    for (const id of order) {
      if (found.has(id) || missing.has(id)) continue
      const built = buildMissing?.(id)
      if (built) found.set(id, built)
      else {
        missing.add(id)
        issues.push(reduceIssue(id, `template: no component with this id`))
      }
    }
    for (const node of [...found.values()]) collect(node)
  }
  if (rootNamed) issues.push(reduceIssue(root.id, `template: cycle through this id`))
  // Cycles: a template that reaches itself through the templates its
  // subtree's owners name. Each round puts the cyclic ones back in place
  // (which may give their new ancestors edges), until none is left.
  const lifted = new Set(found.keys())
  for (;;) {
    for (const [node, orig] of originals) {
      node.children = orig.children.filter((c) => !lifted.has(c.id))
      if (orig.slots) {
        const kept = Object.entries(orig.slots).filter(([, c]) => !lifted.has(c.id))
        if (kept.length > 0) node.slots = Object.fromEntries(kept)
        else delete node.slots
      }
    }
    const deps = new Map<string, Set<string>>()
    for (const id of lifted) {
      const out = new Set<string>()
      const walk = (n: UiNode) => {
        const t = n.template?.component
        if (t !== undefined && lifted.has(t)) out.add(t)
        for (const slot of Object.values(n.slots ?? {})) walk(slot)
        n.children.forEach(walk)
      }
      walk(found.get(id)!)
      deps.set(id, out)
    }
    const reaches = (from: string, to: string): boolean => {
      const seen = new Set<string>()
      const stack = [...(deps.get(from) ?? [])]
      while (stack.length > 0) {
        const id = stack.pop()!
        if (id === to) return true
        if (seen.has(id)) continue
        seen.add(id)
        stack.push(...(deps.get(id) ?? []))
      }
      return false
    }
    const cyclic = order.filter((id) => lifted.has(id) && reaches(id, id))
    if (cyclic.length === 0) break
    for (const id of cyclic) {
      lifted.delete(id)
      issues.push(reduceIssue(id, `template: cycle through this id`))
    }
  }
  const out = new Map<string, UiNode>()
  for (const id of order) {
    const node = found.get(id)
    if (node && lifted.has(id)) out.set(id, node)
  }
  return out
}

/** Expand the root, then each lifted template (one issue list), and cap
 *  each EXPANDED tree's depth right after its expansion (the Rust core's
 *  `finish`): `maxDepth` bounds the reduced tree a host serializes, not the
 *  authored one (a Card is two levels once expanded). */
function finish(root: UiNode, lifted: Map<string, UiNode>, expand: boolean, extensions: readonly ExtensionDef[], catalogId: string | undefined, issues: ReduceIssue[]): ReduceResult {
  // R8 F51: ONE budget over the expanded trees (the root's, then every
  // lifted template's): a macro's `$each` parts count.
  const budget = { left: MAX_COMPONENTS, reported: issues.some((i) => i.message === LIMIT_ISSUES.components) }
  // eApps U3: the items a macro's `$template` parts built join the lifted
  // templates (after them, in build order: the root's first).
  const built: UiNode[] = []
  root = capComponents(capDepth(expand ? expandMacros(root, { extensions, issues, templates: built }) : root, catalogId, issues), budget, issues)
  if (lifted.size === 0 && built.length === 0) return { root, issues }
  const templates: Record<string, UiNode> = {}
  for (const [id, node] of lifted) templates[id] = capComponents(capDepth(expand ? expandMacros(node, { extensions, issues, templates: built }) : node, catalogId, issues), budget, issues)
  for (const node of built) if (!(node.id in templates)) templates[node.id] = capComponents(capDepth(node, catalogId, issues), budget, issues)
  return { root, issues, templates }
}

/** R8 F51: the `maxComponents` budget over an EXPANDED tree (the flat
 *  builder counted the authored nodes; a macro's `$each` multiplies them: a
 *  Rating with `max: 10000` is 20,001 parts). Pre-order (a child, its
 *  subtree, the next child; children before slots); a node past the budget
 *  is dropped with its subtree, the first refusal is the budget issue (once
 *  per surface). Recursive: `capDepth` ran first, so the tree is at most
 *  `maxDepth` levels deep. */
function capComponents(root: UiNode, budget: { left: number; reported: boolean }, issues: ReduceIssue[]): UiNode {
  spend(budget, root.id, issues)
  const walk = (node: UiNode): void => {
    node.children = node.children.filter((child) => {
      if (!spend(budget, child.id, issues)) return false
      walk(child)
      return true
    })
    if (node.slots)
      for (const [name, slot] of Object.entries(node.slots)) {
        if (!spend(budget, slot.id, issues)) delete node.slots[name]
        else walk(slot)
      }
  }
  walk(root)
  return root
}

/** VAPP-103: every node at level MAX_DEPTH + 1 (the tree's root = 1;
 *  children, then slots, one level down) becomes the Unknown placeholder
 *  for its id and component, its subtree dropped, with the depth issue on
 *  its id once (pre-order; an id already reported, e.g. by the authored-
 *  depth check, is not reported again). An Unknown LEAF there is already a
 *  placeholder: kept, never re-reported. Iterative. */
function capDepth(root: UiNode, catalogId: string | undefined, issues: ReduceIssue[]): UiNode {
  const reported = new Set(issues.filter((i) => i.message === LIMIT_ISSUES.depth).map((i) => i.id))
  const stack: { node: UiNode; depth: number }[] = [{ node: root, depth: 1 }]
  while (stack.length) {
    const { node, depth } = stack.pop()!
    const cap = (child: UiNode): UiNode => {
      if (depth + 1 <= MAX_DEPTH) return child
      if (child.component === UNKNOWN_COMPONENT && child.children.length === 0 && child.slots === undefined) return child
      if (!reported.has(child.id)) {
        reported.add(child.id)
        issues.push(reduceIssue(child.id, LIMIT_ISSUES.depth))
      }
      return unknown(child.id, child.component, catalogId)
    }
    const next: UiNode[] = []
    node.children = node.children.map((child) => {
      const capped = cap(child)
      next.push(capped)
      return capped
    })
    if (node.slots)
      for (const [name, slot] of Object.entries(node.slots)) {
        const capped = cap(slot)
        node.slots[name] = capped
        next.push(capped)
      }
    if (depth + 1 > MAX_DEPTH) continue
    for (let i = next.length - 1; i >= 0; i--) stack.push({ node: next[i]!, depth: depth + 1 })
  }
  return root
}

function unknown(id: string, component: string, catalogId: string | undefined): UiNode {
  return { id, component: UNKNOWN_COMPONENT, props: catalogId === undefined ? { component } : { component, catalogId }, children: [] }
}

function ownProps(flat: FlatComponent): Record<string, unknown> {
  const props: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(flat)) {
    if (!STRUCTURAL.has(key) && value !== undefined) props[key] = value
  }
  return props
}

/** Round 9 §5.5: the available set when the caller names none. */
export function defaultAvailable(extensions: readonly ExtensionDef[]): string[] {
  const out: string[] = []
  for (const id of [CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, A2UI_BASIC_CATALOG_ID, ...extensions.map((e) => e.id)]) if (!out.includes(id)) out.push(id)
  return out
}

const ownDef = (table: Record<string, ComponentDef> | undefined, name: string): ComponentDef | undefined =>
  table && Object.prototype.hasOwnProperty.call(table, name) ? table[name] : undefined

/** Round 9 §5.5: a component name looked up ONLY in its resolved catalog:
 *  the core's components; core-lite = the `lite` subset; an extension = its
 *  components ∪ the core's. (Basic components are mapped first.) */
export function catalogComponent(catalogId: string, name: string, extensions: readonly ExtensionDef[]): ComponentDef | undefined {
  if (name === SURFACE_COMPONENT) return undefined
  if (catalogId === CORE_CATALOG_ID) return ownDef(coreCatalog.components, name)
  if (catalogId === CORE_LITE_CATALOG_ID) {
    const def = ownDef(coreCatalog.components, name)
    return def && (def.lite || def.hidden) ? def : undefined
  }
  const ext = extensions.find((e) => e.id === catalogId)
  return ext ? (ownDef(ext.components, name) ?? ownDef(coreCatalog.components, name)) : undefined
}

/** Round 9 §5.6: `allowedChildren: []` when the component takes no children and has no slots. */
export function allowsChildren(def: ComponentDef): boolean {
  return def.children !== `none` || (def.slots ?? []).length > 0
}

/** Round 9 §5.1: the closed accessibility object (unknown keys reported and dropped). */
function accessibilityOf(value: unknown, id: string, issues: ReduceIssue[]): Record<string, unknown> | undefined {
  if (typeof value !== `object` || value === null || Array.isArray(value)) {
    issues.push(reduceIssue(id, `accessibility: expected an object`, `/accessibility`))
    return undefined
  }
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(value)) {
    if (!ACCESSIBILITY_KEYS.includes(k)) {
      issues.push(reduceIssue(id, `accessibility.${k}: unknown key`, `/accessibility/${escapeToken(k)}`))
      continue
    }
    if (k === `live` && !LIVE_VALUES.includes(v as string)) {
      issues.push(reduceIssue(id, `accessibility.live: expected one of ${LIVE_VALUES.join(`|`)}`, `/accessibility/live`))
      continue
    }
    if (ACCESSIBILITY_STATES.includes(k) && typeof v !== `boolean` && !isDynamic(v)) {
      issues.push(reduceIssue(id, `accessibility.${k}: expected a boolean`, `/accessibility/${k}`))
      continue
    }
    out[k] = v
  }
  return Object.keys(out).length > 0 ? out : undefined
}

/** A `visible` value is a boolean or a dynamic value (a binding or a call). */
export function validVisible(value: unknown): value is Visible {
  return typeof value === `boolean` || isDynamic(value)
}

export function childTemplate(value: WireChildTemplate): ChildTemplate {
  const out: ChildTemplate = { component: value.componentId, path: value.path }
  if (typeof value.key === `string`) out.key = value.key
  return out
}

/** Reduce a flat component list. Children are resolved from the root down, so
 *  components nothing references (a basic Button's consumed Text child) do
 *  not appear. A missing or cyclic reference becomes an Unknown placeholder
 *  plus an issue. VAPP-103: an id placed a second time (two parents, or one
 *  parent twice) renders at its first place only (`id used twice`), a node
 *  deeper than `maxDepth` is an Unknown placeholder, and past
 *  `maxComponents` nodes the rest is dropped (catalog/limits.json). */
export function reduceSurface(components: readonly FlatComponent[], options: ReduceOptions): ReduceResult {
  const extensions = options.extensions ?? []
  const available = options.available ?? defaultAvailable(extensions)
  const issues: ReduceIssue[] = []
  const byId = new Map<string, FlatComponent>()
  for (const flat of components) {
    // Round 9 §5.2: the router's entry check drops what is not a component;
    // a direct caller's junk is skipped, never a throw.
    if (!flat || typeof flat !== `object` || typeof flat.id !== `string` || typeof flat.component !== `string`) continue
    if (byId.has(flat.id)) issues.push(reduceIssue(flat.id, `duplicate id; the last definition wins`))
    byId.set(flat.id, flat)
  }
  const lookup = (id: string) => byId.get(id)
  /** Round 9 §5.5: a component's catalog = its own, else the surface default. */
  const catalogOf = (flat: FlatComponent): string | undefined => (typeof flat.catalogId === `string` ? flat.catalogId : options.catalogId)
  const visiting = new Set<string>()
  const placed = new Set<string>()
  // R8 F52: the ids whose second place was already reported (a 4 MB
  // `children: ["x", "x", …]` costs ONE issue, not one per reference).
  const twiceReported = new Set<string>()
  const budget = { left: MAX_COMPONENTS, reported: false }
  /** Round 9 §5.6: the parent each node was built under (`Surface` for the root). */
  const parents = new Map<UiNode, string>()

  // R8 F52: the Unknown placeholders for a missing or cyclic id are placed
  // nodes too, so they spend the budget BEFORE they are made: a message
  // whose children list repeats an unknown id a million times stops at
  // `maxComponents` placeholders (one budget issue).
  const build = (id: string, depth: number): UiNode | undefined => {
    const flat = byId.get(id)
    if (flat) {
      // A cycle (the id is an ancestor) before a second place: the ancestor
      // is in `placed` too.
      if (visiting.has(id)) {
        if (!spend(budget, id, issues)) return undefined
        issues.push(reduceIssue(id, `cycle through this id`))
        return unknown(id, flat.component, catalogOf(flat))
      }
      if (placed.has(id)) {
        if (!twiceReported.has(id)) {
          twiceReported.add(id)
          issues.push(reduceIssue(id, LIMIT_ISSUES.usedTwice))
        }
        return undefined
      }
    }
    if (!spend(budget, id, issues)) return undefined
    if (!flat) {
      issues.push(reduceIssue(id, `no component with this id`))
      return unknown(id, `#${id}`, options.catalogId)
    }
    placed.add(id)
    const catalogId = catalogOf(flat)
    if (depth > MAX_DEPTH) {
      issues.push(reduceIssue(id, LIMIT_ISSUES.depth))
      return unknown(id, flat.component, catalogId)
    }
    const child = (childId: string) => build(childId, depth + 1)
    const children = (ids: readonly string[]) => ids.map(child).filter((n): n is UiNode => n !== undefined)
    visiting.add(id)
    let node: UiNode
    if (catalogId === undefined || !available.includes(catalogId)) {
      issues.push(reduceIssue(id, catalogId === undefined ? `no catalog for this component` : `catalog ${catalogId} is not available`, ``, `UNSUPPORTED_CATALOG`))
      node = unknown(id, flat.component, catalogId)
    } else if (catalogId === A2UI_BASIC_CATALOG_ID) {
      const mapped = mapBasicComponent(flat, lookup, catalogOf)
      if (!mapped) {
        issues.push(reduceIssue(id, `basic component ${flat.component} has no mapping`))
        node = unknown(id, flat.component, catalogId)
      } else {
        node = { id, component: mapped.component, props: mapped.props, children: children(mapped.childrenIds) }
        if (mapped.template) node.template = mapped.template
        if (mapped.style) node.style = mapped.style
        if (mapped.on) node.on = mapped.on
        for (const [slot, childId] of Object.entries(mapped.slots)) {
          const built = child(childId)
          if (built) (node.slots ??= {})[slot] = built
        }
        for (const i of mapped.issues ?? []) issues.push(reduceIssue(id, i.message, `/${escapeToken(i.prop)}`))
      }
    } else {
      const def = catalogComponent(catalogId, flat.component, extensions)
      if (!def) {
        issues.push(reduceIssue(id, `unknown component ${flat.component}`, `/component`))
        node = unknown(id, flat.component, catalogId)
      } else {
        node = { id, component: flat.component, props: ownProps(flat), children: [] }
        if (Array.isArray(flat.children)) node.children = children(flat.children)
        else if (flat.children && typeof flat.children === `object`) node.template = childTemplate(flat.children)
        if (flat.style) node.style = flat.style
        if (flat.on) node.on = flat.on
        for (const [slot, childId] of Object.entries(flat.slots ?? {})) {
          const built = child(childId)
          if (built) (node.slots ??= {})[slot] = built
        }
      }
    }
    for (const c of node.children) parents.set(c, flat.component)
    for (const c of Object.values(node.slots ?? {})) parents.set(c, flat.component)
    // `visible`, `accessibility` and `metadata` hold on EVERY component, basic or core.
    if (flat.visible !== undefined) {
      if (validVisible(flat.visible)) node.visible = flat.visible
      else issues.push(reduceIssue(id, `visible: expected a boolean, a binding or a function call`, `/visible`))
    }
    if (flat.accessibility !== undefined) {
      const a11y = accessibilityOf(flat.accessibility, id, issues)
      if (a11y) node.accessibility = a11y
    }
    if (flat.metadata && typeof flat.metadata === `object`) node.metadata = flat.metadata
    visiting.delete(id)
    if (options.validate !== false && node.component !== UNKNOWN_COMPONENT) validateBuilt(node, id, extensions, issues)
    return node
  }

  const rootId = options.rootId ?? `root`
  let root = build(rootId, 1) ?? unknown(rootId, `#${rootId}`, options.catalogId)
  parents.set(root, SURFACE_COMPONENT)
  const lifted = liftTemplates(root, issues, (id) => (byId.has(id) && !placed.has(id) ? build(id, 1) : undefined))
  if (options.validate !== false) checkParents(root, lifted, parents, extensions, issues)
  return finish(root, lifted, options.expand !== false, extensions, options.catalogId, issues)
}

/** The catalog checks on a built node (props, children, slots, calls), each
 *  issue with its code and pointer. */
function validateBuilt(node: UiNode, id: string, extensions: readonly ExtensionDef[], issues: ReduceIssue[]): void {
  const view = sharedCatalogView(extensions)
  const def = view.components[node.component]
  if (!def) return
  for (const issue of validateProps(def, node.props, { path: `props`, extensions })) issues.push(reduceIssue(id, `${issue.path}: ${issue.message}`, issuePointer(issue.path)))
  if (!allowsChildren(def) && node.children.length > 0) issues.push(reduceIssue(id, `${node.component} takes no children`, `/children`, `UNALLOWED_CHILD`))
  for (const slot of Object.keys(node.slots ?? {}))
    if (!slotAllowed(def.slots, slot)) issues.push(reduceIssue(id, `slots.${slot}: ${node.component} has no such slot`, `/slots/${escapeToken(slot)}`))
  for (const issue of validateNode(node, { extensions })) issues.push(reduceIssue(id, `${issue.path}: ${issue.message}`, issuePointer(issue.path)))
}

/** Round 9 §5.6: every node against its own `allowedParents` (catalog
 *  `parents`; `Surface` = the root's parent). */
function checkParents(root: UiNode, lifted: Map<string, UiNode>, parents: Map<UiNode, string>, extensions: readonly ExtensionDef[], issues: ReduceIssue[]): void {
  const view = sharedCatalogView(extensions)
  const walk = (n: UiNode) => {
    const allowed = view.components[n.component]?.parents
    const parent = parents.get(n)
    if (allowed && parent !== undefined && !allowed.includes(parent))
      issues.push(reduceIssue(n.id, `${n.component} cannot be a child of ${parent}; allowed parents: ${allowed.join(`|`)}`, ``, `UNALLOWED_PARENT`))
    for (const s of Object.values(n.slots ?? {})) walk(s)
    n.children.forEach(walk)
  }
  walk(root)
  for (const t of lifted.values()) walk(t)
}

/** Take one node from the surface's budget; the first refusal is an issue. */
function spend(budget: { left: number; reported: boolean }, id: string, issues: ReduceIssue[]): boolean {
  if (budget.left > 0) {
    budget.left -= 1
    return true
  }
  if (!budget.reported) issues.push(reduceIssue(id, LIMIT_ISSUES.components))
  budget.reported = true
  return false
}

/** A component's slot list admits a name when it lists it or lists `*`. */
export function slotAllowed(slots: readonly string[] | undefined, name: string): boolean {
  return (slots ?? []).some((s) => s === `*` || s === name)
}

/** The nested authoring form → a normalized tree (same validation and
 *  expansion as reduceSurface). */
export function reduceNested(tree: NestedNode, options: Omit<ReduceOptions, `rootId`>): ReduceResult {
  const extensions = options.extensions ?? []
  const available = options.available ?? defaultAvailable(extensions)
  const issues: ReduceIssue[] = []
  const budget = { left: MAX_COMPONENTS, reported: false }
  const parents = new Map<UiNode, string>()
  const walk = (n: NestedNode, depth: number): UiNode | undefined => {
    if (!spend(budget, n.id, issues)) return undefined
    const catalogId = typeof n.catalogId === `string` ? n.catalogId : options.catalogId
    if (depth > MAX_DEPTH) {
      issues.push(reduceIssue(n.id, LIMIT_ISSUES.depth))
      return unknown(n.id, n.component, catalogId)
    }
    if (catalogId === undefined || !available.includes(catalogId)) {
      issues.push(reduceIssue(n.id, catalogId === undefined ? `no catalog for this component` : `catalog ${catalogId} is not available`, ``, `UNSUPPORTED_CATALOG`))
      return unknown(n.id, n.component, catalogId)
    }
    const def = catalogComponent(catalogId, n.component, extensions)
    if (!def) {
      issues.push(reduceIssue(n.id, `unknown component ${n.component}`, `/component`))
      return unknown(n.id, n.component, catalogId)
    }
    const children = (n.children ?? []).map((c) => walk(c, depth + 1)).filter((c): c is UiNode => c !== undefined)
    const node: UiNode = { id: n.id, component: n.component, props: { ...(n.props ?? {}) }, children }
    for (const c of children) parents.set(c, n.component)
    if (n.style) node.style = n.style
    if (n.visible !== undefined) {
      if (validVisible(n.visible)) node.visible = n.visible
      else issues.push(reduceIssue(n.id, `visible: expected a boolean, a binding or a function call`, `/visible`))
    }
    if (n.on) node.on = n.on
    if (n.accessibility !== undefined) {
      const a11y = accessibilityOf(n.accessibility, n.id, issues)
      if (a11y) node.accessibility = a11y
    }
    if (n.metadata && typeof n.metadata === `object`) node.metadata = n.metadata
    if (n.template) node.template = n.template
    if (n.slots) {
      node.slots = {}
      for (const [slot, child] of Object.entries(n.slots)) {
        const built = walk(child, depth + 1)
        if (built) {
          node.slots[slot] = built
          parents.set(built, n.component)
        }
      }
    }
    if (options.validate !== false) validateBuilt(node, n.id, extensions, issues)
    return node
  }
  let root = walk(tree, 1)!
  parents.set(root, SURFACE_COMPONENT)
  const lifted = liftTemplates(root, issues)
  if (options.validate !== false) checkParents(root, lifted, parents, extensions, issues)
  return finish(root, lifted, options.expand !== false, extensions, options.catalogId, issues)
}

/** Round 9 §13: a card as 1.0: `"version": "1.0"` passes, no `version` is the
 *  0.9 form (upgraded while the bridge is on), anything else is refused. */
export function cardV10(card: unknown, acceptV09: boolean = ACCEPT_V09): { card: { version: string; root: NestedNode; data?: unknown } } | { error: string } {
  if (typeof card !== `object` || card === null || Array.isArray(card)) return { error: `a card is a JSON object` }
  const version = (card as { version?: unknown }).version
  if (version === undefined) return acceptV09 ? { card: upgradeCard09(card) as { version: string; root: NestedNode } } : { error: `version is required: write "version": "1.0"` }
  if (version === A2UI_VERSION) return { card: card as { version: string; root: NestedNode } }
  return { error: `unsupported version ${typeof version === `string` ? version : JSON.stringify(version)}` }
}

/** Round 9 §13: what a card refuses beyond `reduceNested`'s issues: the entry
 *  check on every node (§5.2) and any name no card can reach (no agent
 *  channel, §6.5). Each `<node id>: <reason>`. */
export function cardProblems(root: NestedNode): string[] {
  const out: string[] = []
  const known = (v: unknown): void => {
    if (Array.isArray(v)) return v.forEach(known)
    if (typeof v !== `object` || v === null) return
    const call = (v as { "@call"?: unknown })[`@call`]
    if (typeof call === `string` && !isKnownFunctionName(call)) out.push(`unknown function ${call}`)
    Object.values(v).forEach(known)
  }
  const walk = (n: NestedNode): void => {
    const { props, children, slots, template, ...rest } = n as NestedNode & { template?: unknown }
    const failure = checkEntry({ ...rest, ...(props ?? {}) }, { catalogId: CORE_CATALOG_ID })
    if (failure) out.push(`${String(n.id)}: ${failure.pointer || `/`} ${failure.message}`)
    known(props)
    known(n.on)
    known(n.visible)
    known(n.accessibility)
    if (Array.isArray(children)) children.forEach(walk)
    for (const child of Object.values(slots ?? {})) walk(child)
  }
  walk(root)
  return out
}

/** Pre-order ids of a tree, the painters' accessibility order. */
export function preorder(root: UiNode): string[] {
  const out: string[] = []
  const walk = (n: UiNode) => {
    out.push(n.id)
    for (const slot of Object.values(n.slots ?? {})) walk(slot)
    n.children.forEach(walk)
  }
  walk(root)
  return out
}
