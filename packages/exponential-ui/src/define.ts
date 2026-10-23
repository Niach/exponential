// VAPP-108: the typed authoring DSL for humans. `defineSurface` takes a
// nested, TYPED tree (every component's props autocomplete from the catalog
// types generate.ts emits into catalog.generated.ts) and returns the flat
// A2UI list, the messages a host consumes, and the validator's issues: the
// same reducer an agent's JSON goes through, so what compiles is what
// renders ×4. No runtime dependency beyond the reducer.
//
//   const surface = defineSurface(
//     ui.Stack({ gap: "md" }, [
//       ui.Section({ title: "Devices", icon: "nav-devices" }),
//       ui.List({ divided: true }, { template: ui.Row({ title: bind("label") }), path: "/devices/rows", key: "id" }),
//     ]),
//     { surfaceId: "devices" }
//   )
//   surface.issues        // [] when valid
//   surface.components    // the flat list (updateComponents)
//   surface.messages      // createSurface + updateComponents (+ updateDataModel)
//   surface.jsonl         // what an agent would send

import { COMPONENT_NAMES, type ComponentName, type ComponentProps } from "./catalog.generated"
import { CORE_CATALOG_ID } from "./catalog"
import { reduceSurface } from "./reducer"
import type { Action, AuthoredAccessibility, DataBinding, ExtensionDef, FlatComponent, FunctionCall, ReduceIssue, UiNode, Visible } from "./types"

/** A node of the typed tree: `component` picks the props type. */
export interface SurfaceNode<C extends ComponentName = ComponentName> {
  /** Explicit id; `defineSurface` assigns `root` to the top node and `<component>-<n>` to the rest. */
  id?: string
  component: C
  props: ComponentProps[C]
  style?: Record<string, unknown>
  on?: Record<string, Action>
  visible?: Visible
  accessibility?: AuthoredAccessibility
  /** Static children, or one template rendered per item of `path`. */
  children?: SurfaceNode[] | TemplateChildren
  slots?: Record<string, SurfaceNode>
}

export interface TemplateChildren {
  template: SurfaceNode
  /** The JSON pointer of the array. */
  path: string
  /** The item field that identifies it. */
  key?: string
}

export interface DefineOptions {
  surfaceId?: string
  catalogId?: string
  extensions?: readonly ExtensionDef[]
  /** Initial data model writes: pointer → value (one updateDataModel each). */
  data?: Record<string, unknown>
}

export interface DefinedSurface {
  surfaceId: string
  catalogId: string
  components: FlatComponent[]
  issues: ReduceIssue[]
  /** The normalized tree (macros expanded), for a host that paints directly. */
  root: UiNode
  messages: Record<string, unknown>[]
  jsonl: string
}

/** A data binding: `{ path }`. A pointer relative to the item inside a template. */
export const bind = (path: string): DataBinding => ({ path })
/** A client function call: `{ call, args }`. */
export const call = (name: string, args?: Record<string, unknown>): FunctionCall => ({ call: name, ...(args ? { args } : {}) })
/** An action that sends an event to the agent / server. */
export const event = (name: string, context?: Record<string, unknown>): Action => ({ event: { name, ...(context ? { context } : {}) } })
/** An action that runs a client function (`set`, `openUrl`, `app.toast`…). */
export const run = (name: string, args?: Record<string, unknown>): Action => ({ functionCall: call(name, args) })

type Extra<C extends ComponentName> = Omit<SurfaceNode<C>, `component` | `props` | `children`>

/** The typed factory per component: `ui.Stack(props, children?, extra?)`. */
export type Factory<C extends ComponentName> = (props: ComponentProps[C], children?: SurfaceNode[] | TemplateChildren, extra?: Extra<C>) => SurfaceNode<C>

/** `ui.Card({ title: "…" }, [ui.Text({ text: "…" })])`: one factory per
 *  catalog component, typed by name. Props only: style, on, visible,
 *  slots and an explicit id go in the third argument. */
export const ui: { [C in ComponentName]: Factory<C> } = Object.fromEntries(
  COMPONENT_NAMES.map((name) => [name, ((props: ComponentProps[typeof name], children?: SurfaceNode[] | TemplateChildren, extra?: Extra<typeof name>) => ({ component: name, props, ...(children ? { children } : {}), ...extra })) as Factory<typeof name>])
) as { [C in ComponentName]: Factory<C> }

/** A typed node from a name (for a component picked at runtime). */
export function el<C extends ComponentName>(component: C, props: ComponentProps[C], children?: SurfaceNode[] | TemplateChildren, extra?: Extra<C>): SurfaceNode<C> {
  return { component, props, ...(children ? { children } : {}), ...extra }
}

/** The tree → the flat list, ids assigned (the top node is `root`; an
 *  unnamed node is `<component lower-case>-<n>`, unique within the tree). */
export function flattenSurface(tree: SurfaceNode, rootId = `root`): FlatComponent[] {
  const out: FlatComponent[] = []
  const taken = new Set<string>()
  const counters = new Map<string, number>()
  const nameOf = (n: SurfaceNode, isRoot: boolean): string => {
    if (n.id) {
      if (taken.has(n.id)) throw new Error(`defineSurface: duplicate id "${n.id}"`)
      taken.add(n.id)
      return n.id
    }
    if (isRoot && !taken.has(rootId)) {
      taken.add(rootId)
      return rootId
    }
    const base = n.component.charAt(0).toLowerCase() + n.component.slice(1)
    let id: string
    do {
      const next = (counters.get(base) ?? 0) + 1
      counters.set(base, next)
      id = `${base}-${next}`
    } while (taken.has(id))
    taken.add(id)
    return id
  }
  const walk = (n: SurfaceNode, isRoot: boolean): string => {
    const id = nameOf(n, isRoot)
    const flat: FlatComponent = { id, component: n.component, ...(n.props as Record<string, unknown>) }
    if (n.style) flat.style = n.style
    if (n.on) flat.on = n.on
    if (n.visible !== undefined) flat.visible = n.visible
    if (n.accessibility) flat.accessibility = n.accessibility
    out.push(flat)
    if (Array.isArray(n.children)) flat.children = n.children.map((c) => walk(c, false))
    else if (n.children) {
      const t = walk(n.children.template, false)
      flat.children = { componentId: t, path: n.children.path, ...(n.children.key ? { key: n.children.key } : {}) }
    }
    if (n.slots) flat.slots = Object.fromEntries(Object.entries(n.slots).map(([slot, child]) => [slot, walk(child, false)]))
    return id
  }
  walk(tree, true)
  return out
}

/** The typed tree → everything a host or an agent transport needs. Never
 *  throws on a bad surface: read `issues`. */
export function defineSurface(tree: SurfaceNode, options: DefineOptions = {}): DefinedSurface {
  const surfaceId = options.surfaceId ?? `main`
  const catalogId = options.catalogId ?? CORE_CATALOG_ID
  const components = flattenSurface(tree)
  const { root, issues } = reduceSurface(components, { catalogId, extensions: options.extensions })
  const messages: Record<string, unknown>[] = [
    { version: `v0.9`, createSurface: { surfaceId, catalogId } },
    { version: `v0.9`, updateComponents: { surfaceId, components } },
    ...Object.entries(options.data ?? {}).map(([path, value]) => ({ version: `v0.9`, updateDataModel: { surfaceId, path, value } })),
  ]
  return { surfaceId, catalogId, components, issues, root, messages, jsonl: messages.map((m) => JSON.stringify(m)).join(`\n`) }
}
