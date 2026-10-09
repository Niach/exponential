// VAPP-85: the TypeScript REFERENCE reducer. A surface's flat component list
// (A2UI `updateComponents`, core OR basic catalog) → ONE normalized tree in
// the core vocabulary, macros expanded, every unknown component the explicit
// `Unknown` placeholder. The Rust core (VAPP-86) replays the same fixtures.

import {
  A2UI_BASIC_CATALOG_ID,
  CORE_CATALOG_ID,
  CORE_LITE_CATALOG_ID,
  UNKNOWN_COMPONENT,
  catalogView,
} from "./catalog"
import { mapBasicComponent } from "./basic-map"
import { isDynamic } from "./expr"
import { expandMacros } from "./macros"
import { validateNode, validateProps } from "./validate"
import { LIMIT_ISSUES, MAX_COMPONENTS, MAX_DEPTH } from "./limits"
import type {
  ChildTemplate,
  ExtensionDef,
  FlatComponent,
  NestedNode,
  ReduceIssue,
  UiNode,
  Visible,
  WireChildTemplate,
} from "./types"

/** The keys of a flat component that are not props (round 1 adds `visible`). */
export const RESERVED_KEYS: readonly string[] = [`id`, `component`, `children`, `slots`, `on`, `style`, `visible`, `accessibility`, `template`]
const RESERVED = new Set(RESERVED_KEYS)

export interface ReduceOptions {
  /** The surface's catalog: the core, the lite subset, the basic catalog or an extension's id. */
  catalogId: string
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
        issues.push({ id, message: `template: no component with this id` })
      }
    }
    for (const node of [...found.values()]) collect(node)
  }
  if (rootNamed) issues.push({ id: root.id, message: `template: cycle through this id` })
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
      issues.push({ id, message: `template: cycle through this id` })
    }
  }
  const out = new Map<string, UiNode>()
  for (const id of order) {
    const node = found.get(id)
    if (node && lifted.has(id)) out.set(id, node)
  }
  return out
}

/** The template table expanded like the root (macros, same issue list). */
function finishTemplates(lifted: Map<string, UiNode>, expand: boolean, extensions: readonly ExtensionDef[], issues: ReduceIssue[]): Record<string, UiNode> | undefined {
  if (lifted.size === 0) return undefined
  const out: Record<string, UiNode> = {}
  for (const [id, node] of lifted) out[id] = expand ? expandMacros(node, { extensions, issues }) : node
  return out
}

function unknown(id: string, component: string, catalogId: string): UiNode {
  return { id, component: UNKNOWN_COMPONENT, props: { component, catalogId }, children: [] }
}

function ownProps(flat: FlatComponent): Record<string, unknown> {
  const props: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(flat)) {
    if (!RESERVED.has(key) && value !== undefined) props[key] = value
  }
  return props
}

function knownCatalog(catalogId: string, extensions: readonly ExtensionDef[]): boolean {
  return (
    catalogId === CORE_CATALOG_ID ||
    catalogId === CORE_LITE_CATALOG_ID ||
    catalogId === A2UI_BASIC_CATALOG_ID ||
    extensions.some((ext) => ext.id === catalogId)
  )
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
  const view = catalogView(extensions)
  const issues: ReduceIssue[] = []
  const byId = new Map<string, FlatComponent>()
  for (const flat of components) {
    if (byId.has(flat.id)) issues.push({ id: flat.id, message: `duplicate id; the last definition wins` })
    byId.set(flat.id, flat)
  }
  const lookup = (id: string) => byId.get(id)
  const basic = options.catalogId === A2UI_BASIC_CATALOG_ID
  if (!knownCatalog(options.catalogId, extensions))
    issues.push({ id: options.rootId ?? `root`, message: `unsupported catalog ${options.catalogId}` })
  const visiting = new Set<string>()
  const placed = new Set<string>()
  const budget = { left: MAX_COMPONENTS, reported: false }

  const build = (id: string, depth: number): UiNode | undefined => {
    const flat = byId.get(id)
    if (!flat) {
      issues.push({ id, message: `no component with this id` })
      return unknown(id, `#${id}`, options.catalogId)
    }
    if (visiting.has(id)) {
      issues.push({ id, message: `cycle through this id` })
      return unknown(id, flat.component, options.catalogId)
    }
    if (placed.has(id)) {
      issues.push({ id, message: LIMIT_ISSUES.usedTwice })
      return undefined
    }
    if (!spend(budget, id, issues)) return undefined
    placed.add(id)
    if (depth > MAX_DEPTH) {
      issues.push({ id, message: LIMIT_ISSUES.depth })
      return unknown(id, flat.component, options.catalogId)
    }
    const child = (childId: string) => build(childId, depth + 1)
    const children = (ids: readonly string[]) => ids.map(child).filter((n): n is UiNode => n !== undefined)
    visiting.add(id)
    let node: UiNode
    if (basic) {
      const mapped = mapBasicComponent(flat, lookup)
      if (!mapped) {
        issues.push({ id, message: `basic component ${flat.component} has no mapping` })
        node = unknown(id, flat.component, options.catalogId)
      } else {
        node = { id, component: mapped.component, props: mapped.props, children: children(mapped.childrenIds) }
        if (mapped.template) node.template = mapped.template
        if (mapped.style) node.style = mapped.style
        if (mapped.on) node.on = mapped.on
        for (const [slot, childId] of Object.entries(mapped.slots)) {
          const built = child(childId)
          if (built) (node.slots ??= {})[slot] = built
        }
      }
    } else {
      const def = view.components[flat.component]
      if (!def) {
        issues.push({ id, message: `unknown component ${flat.component}` })
        node = unknown(id, flat.component, options.catalogId)
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
    // `visible` and `accessibility` hold on EVERY component, basic or core.
    if (flat.visible !== undefined) {
      if (validVisible(flat.visible)) node.visible = flat.visible
      else issues.push({ id, message: `visible: expected a boolean, a binding or a function call` })
    }
    if (flat.accessibility) node.accessibility = flat.accessibility
    visiting.delete(id)
    if (options.validate !== false && node.component !== UNKNOWN_COMPONENT) {
      const def = view.components[node.component]
      if (def) {
        for (const issue of validateProps(def, node.props, { path: `props`, extensions }))
          issues.push({ id, message: `${issue.path}: ${issue.message}` })
        if (def.children === `none` && node.children.length > 0)
          issues.push({ id, message: `${node.component} takes no children` })
        for (const slot of Object.keys(node.slots ?? {}))
          if (!slotAllowed(def.slots, slot)) issues.push({ id, message: `slots.${slot}: ${node.component} has no such slot` })
        for (const issue of validateNode(node)) issues.push({ id, message: `${issue.path}: ${issue.message}` })
      }
    }
    return node
  }

  const rootId = options.rootId ?? `root`
  let root = build(rootId, 1) ?? unknown(rootId, `#${rootId}`, options.catalogId)
  const lifted = liftTemplates(root, issues, (id) => (byId.has(id) && !placed.has(id) ? build(id, 1) : undefined))
  if (options.expand !== false) root = expandMacros(root, { extensions, issues })
  const templates = finishTemplates(lifted, options.expand !== false, extensions, issues)
  return templates ? { root, issues, templates } : { root, issues }
}

/** Take one node from the surface's budget; the first refusal is an issue. */
function spend(budget: { left: number; reported: boolean }, id: string, issues: ReduceIssue[]): boolean {
  if (budget.left > 0) {
    budget.left -= 1
    return true
  }
  if (!budget.reported) issues.push({ id, message: LIMIT_ISSUES.components })
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
  const view = catalogView(extensions)
  const issues: ReduceIssue[] = []
  const budget = { left: MAX_COMPONENTS, reported: false }
  const walk = (n: NestedNode, depth: number): UiNode | undefined => {
    if (!spend(budget, n.id, issues)) return undefined
    if (depth > MAX_DEPTH) {
      issues.push({ id: n.id, message: LIMIT_ISSUES.depth })
      return unknown(n.id, n.component, options.catalogId)
    }
    const def = view.components[n.component]
    if (!def) {
      issues.push({ id: n.id, message: `unknown component ${n.component}` })
      return unknown(n.id, n.component, options.catalogId)
    }
    const children = (n.children ?? []).map((c) => walk(c, depth + 1)).filter((c): c is UiNode => c !== undefined)
    const node: UiNode = { id: n.id, component: n.component, props: { ...(n.props ?? {}) }, children }
    if (n.style) node.style = n.style
    if (n.visible !== undefined) {
      if (validVisible(n.visible)) node.visible = n.visible
      else issues.push({ id: n.id, message: `visible: expected a boolean, a binding or a function call` })
    }
    if (n.on) node.on = n.on
    if (n.accessibility) node.accessibility = n.accessibility
    if (n.template) node.template = n.template
    if (n.slots) {
      node.slots = {}
      for (const [slot, child] of Object.entries(n.slots)) {
        const built = walk(child, depth + 1)
        if (built) node.slots[slot] = built
      }
    }
    if (options.validate !== false) {
      for (const issue of validateProps(def, node.props, { path: `props`, extensions }))
        issues.push({ id: n.id, message: `${issue.path}: ${issue.message}` })
      if (def.children === `none` && node.children.length > 0)
        issues.push({ id: n.id, message: `${n.component} takes no children` })
      for (const slot of Object.keys(node.slots ?? {}))
        if (!slotAllowed(def.slots, slot)) issues.push({ id: n.id, message: `slots.${slot}: ${n.component} has no such slot` })
      for (const issue of validateNode(node)) issues.push({ id: n.id, message: `${issue.path}: ${issue.message}` })
    }
    return node
  }
  let root = walk(tree, 1)!
  const lifted = liftTemplates(root, issues)
  if (options.expand !== false) root = expandMacros(root, { extensions, issues })
  const templates = finishTemplates(lifted, options.expand !== false, extensions, issues)
  return templates ? { root, issues, templates } : { root, issues }
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
