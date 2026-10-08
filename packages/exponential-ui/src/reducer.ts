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
import { validateProps } from "./validate"
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
 *  plus an issue. */
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

  const build = (id: string): UiNode => {
    const flat = byId.get(id)
    if (!flat) {
      issues.push({ id, message: `no component with this id` })
      return unknown(id, `#${id}`, options.catalogId)
    }
    if (visiting.has(id)) {
      issues.push({ id, message: `cycle through this id` })
      return unknown(id, flat.component, options.catalogId)
    }
    visiting.add(id)
    let node: UiNode
    if (basic) {
      const mapped = mapBasicComponent(flat, lookup)
      if (!mapped) {
        issues.push({ id, message: `basic component ${flat.component} has no mapping` })
        node = unknown(id, flat.component, options.catalogId)
      } else {
        node = { id, component: mapped.component, props: mapped.props, children: mapped.childrenIds.map(build) }
        if (mapped.template) node.template = mapped.template
        if (mapped.style) node.style = mapped.style
        if (mapped.on) node.on = mapped.on
        for (const [slot, childId] of Object.entries(mapped.slots)) (node.slots ??= {})[slot] = build(childId)
      }
    } else {
      const def = view.components[flat.component]
      if (!def) {
        issues.push({ id, message: `unknown component ${flat.component}` })
        node = unknown(id, flat.component, options.catalogId)
      } else {
        node = { id, component: flat.component, props: ownProps(flat), children: [] }
        if (Array.isArray(flat.children)) node.children = flat.children.map(build)
        else if (flat.children && typeof flat.children === `object`) node.template = childTemplate(flat.children)
        if (flat.style) node.style = flat.style
        if (flat.on) node.on = flat.on
        for (const [slot, childId] of Object.entries(flat.slots ?? {})) (node.slots ??= {})[slot] = build(childId)
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
      }
    }
    return node
  }

  let root = build(options.rootId ?? `root`)
  if (options.expand !== false) root = expandMacros(root, { extensions, issues })
  return { root, issues }
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
  const walk = (n: NestedNode): UiNode => {
    const def = view.components[n.component]
    if (!def) {
      issues.push({ id: n.id, message: `unknown component ${n.component}` })
      return unknown(n.id, n.component, options.catalogId)
    }
    const node: UiNode = { id: n.id, component: n.component, props: { ...(n.props ?? {}) }, children: (n.children ?? []).map(walk) }
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
      for (const [slot, child] of Object.entries(n.slots)) node.slots[slot] = walk(child)
    }
    if (options.validate !== false) {
      for (const issue of validateProps(def, node.props, { path: `props`, extensions }))
        issues.push({ id: n.id, message: `${issue.path}: ${issue.message}` })
      if (def.children === `none` && node.children.length > 0)
        issues.push({ id: n.id, message: `${n.component} takes no children` })
      for (const slot of Object.keys(node.slots ?? {}))
        if (!slotAllowed(def.slots, slot)) issues.push({ id: n.id, message: `slots.${slot}: ${n.component} has no such slot` })
    }
    return node
  }
  let root = walk(tree)
  if (options.expand !== false) root = expandMacros(root, { extensions, issues })
  return { root, issues }
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
