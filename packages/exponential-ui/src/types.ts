// VAPP-85: the TypeScript shapes of the catalog source, of a normalized
// surface node and of an extension. The JSON files under catalog/ are the
// truth; these types describe them so the reducer, the expander and the
// generator agree.

export type PropType =
  | `string`
  | `markdown`
  | `number`
  | `boolean`
  | `enum`
  | `icon`
  | `color`
  | `style`
  | `array`
  | `object`
  | `date`
  | `url`

export interface PropSchema {
  type: PropType
  description: string
  required?: boolean
  default?: unknown
  /** Accepts an A2UI data binding `{path}` or a function call instead of a literal. */
  bindable?: boolean
  /** `enum`: the name of a shared list in `enums`, or … */
  enum?: string
  /** … an inline list of values. */
  values?: readonly (string | number)[]
  /** `array`: the item schema. */
  items?: PropSchema
  /** `object`: the name of a shared shape in `defs`. */
  shape?: string
}

export interface DefSchema {
  description: string
  properties: Record<string, PropSchema>
}

export type ComponentKind = `native` | `macro`
export type ChildrenRule = `none` | `one` | `many`

export interface ComponentDef {
  kind: ComponentKind
  group: string
  lite: boolean
  children: ChildrenRule
  slots?: readonly string[]
  /** Never offered to a model (the Unknown placeholder). */
  hidden?: boolean
  /** `planned` = in the catalog, painters land later (Chart). */
  status?: string
  description: string
  props: Record<string, PropSchema>
  events?: readonly string[]
  example?: Record<string, unknown>
}

export interface CatalogSource {
  id: string
  liteId: string
  name: string
  version: string
  unknownComponent: string
  enums: Record<string, readonly string[]>
  defs: Record<string, DefSchema>
  functions: { names: readonly string[] }
  components: Record<string, ComponentDef>
}

/** A macro template node (catalog/macros.json). */
export interface MacroTemplate {
  part: string
  component: string
  props?: Record<string, unknown>
  style?: Record<string, unknown>
  children?: (MacroTemplate | `$children`)[]
  slots?: Record<string, string>
  $if?: string | string[] | Record<string, unknown>
  $any?: string[]
  $each?: string
  $as?: string
  $on?: Record<string, string>
  $context?: Record<string, unknown>
  $recipe?: Record<string, unknown>
}

export interface MacroDef {
  recipeProps: readonly string[]
  root: MacroTemplate
}

/** An extension catalog: its own id, extending the core, with components and
 *  the macro templates for its macro components. */
export interface ExtensionDef {
  id: string
  name: string
  extends: string
  enums?: Record<string, readonly string[]>
  defs?: Record<string, DefSchema>
  components: Record<string, ComponentDef>
  macros?: Record<string, MacroDef>
}

/** An A2UI v0.9 data binding or function call, passed through untouched. */
export interface DataBinding {
  path: string
}
export interface FunctionCall {
  call: string
  args?: Record<string, unknown>
  returnType?: string
}
/** An A2UI v0.9 Action: a server event or a client function. */
export interface Action {
  event?: { name: string; context?: Record<string, unknown> }
  function?: FunctionCall
}

/** A node as it rides A2UI's `updateComponents`: props at the top level,
 *  children by id. The core catalog keeps that convention. */
export interface FlatComponent {
  id: string
  component: string
  children?: string[] | { componentId: string; path: string }
  slots?: Record<string, string>
  on?: Record<string, Action>
  style?: Record<string, unknown>
  accessibility?: { label?: unknown; description?: unknown }
  [prop: string]: unknown
}

/** A node of the NORMALIZED tree every painter reads: core vocabulary, props
 *  under `props`, children nested, macros expanded (`recipe` names the macro
 *  part a theme styles). */
export interface UiNode {
  id: string
  component: string
  props: Record<string, unknown>
  style?: Record<string, unknown>
  on?: Record<string, Action>
  accessibility?: { label?: unknown; description?: unknown }
  children: UiNode[]
  slots?: Record<string, UiNode>
  /** A data-driven child list: one `component` per item at `path`. */
  template?: { component: string; path: string }
  recipe?: { macro: string; part: string; props: Record<string, unknown> }
}

/** The nested authoring form (fixtures, kitchen sink): the same as UiNode
 *  minus what the reducer fills in. */
export interface NestedNode {
  id: string
  component: string
  props?: Record<string, unknown>
  style?: Record<string, unknown>
  on?: Record<string, Action>
  accessibility?: { label?: unknown; description?: unknown }
  children?: NestedNode[]
  slots?: Record<string, NestedNode>
  template?: { component: string; path: string }
}

export interface ReduceIssue {
  id: string
  message: string
}
