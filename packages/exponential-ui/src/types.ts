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
  /** Round 1: ALSO accepts `{base, sm?, md?, lg?, xl?}` (keys = the
   *  `$breakpoint` token names); the macro expander emits the media blocks. */
  responsive?: boolean
  /** `enum`: the name of a shared list in `enums`, or … */
  enum?: string
  /** … an inline list of values. */
  values?: readonly (string | number)[]
  /** `array`: the item schema. */
  items?: PropSchema
  /** `object`: the name of a shared shape in `defs`; none = any object. */
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
  /** Named child slots; `["*"]` = any slot name (Table cell templates). */
  slots?: readonly string[]
  /** Round 1: `row` = every slot is a per-ROW cell template (Table): the bind
   *  pass leaves it unbound and the painter binds it once per row with the
   *  row as its data scope (src/dynamic.ts bindRowSlot). */
  slotScope?: `row`
  /** Never offered to a model (the Unknown placeholder, TreeGuides = a Row part). */
  hidden?: boolean
  /** Round 3 (docs/round-3-contract.md): a ONE-RELEASE alias naming its
   *  replacement; a macro that expands to it. Still reduces; never in the
   *  prompt, the docs, the specimens or the lite subset. */
  deprecated?: string
  /** `planned` = in the catalog, painters land later. */
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
  functions: { names: readonly string[]; core: Record<string, CoreFunctionDef> }
  /** Round 1: `<Component>.<part>[.<variant>]` → the icons.json name a
   *  renderer draws for a part it owns (`$comment` aside). */
  builtinIcons: Record<string, string>
  components: Record<string, ComponentDef>
}

/** Round 1: one core function (core.catalog.json `functions.core`). */
export interface CoreFunctionDef {
  description: string
  /** Argument name → its type (`number`, `string`, `boolean`, `array`, `object`, `any`). */
  args: Record<string, string>
  returns: string
}

/** A child entry of a macro template: a part, the author's children
 *  (`$children`) or one of the author's slots spliced in place (`$slot:name`). */
export type MacroChild = MacroTemplate | `$children` | `$slot:${string}`

/** A macro template node (catalog/macros.json). */
export interface MacroTemplate {
  part: string
  component: string
  props?: Record<string, unknown>
  style?: Record<string, unknown>
  children?: MacroChild[]
  /** `$slot:name` copies the author's slot; a template node builds a part. */
  slots?: Record<string, string | MacroTemplate>
  $if?: unknown
  $any?: unknown[]
  $each?: string
  $as?: string
  $on?: Record<string, string>
  $context?: Record<string, unknown>
  $recipe?: Record<string, unknown>
  /** Round 1 two-way binding: per part event, the macro prop a bound author
   *  value is written back to (`set`) and the template value it takes. */
  $set?: Record<string, { prop: string; value: unknown }>
  /** Round 1 (docs/round-1-contract.md §6): the part's accessibility — a
   *  machine-readable `role` (catalog/a11y.json `roles`), its states and an
   *  accessible name — evaluated like any template value (bound inputs emit
   *  calls) into the expanded node's `accessibility`. */
  $a11y?: Record<string, unknown>
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
/** An A2UI v0.9 Action: a server event or a client function. Round 1: the
 *  expander may emit BOTH (a routed event plus a `set` write); a renderer
 *  evaluates the function args and the event context first, then runs the
 *  function, then dispatches the event. */
export interface Action {
  event?: { name: string; context?: Record<string, unknown> }
  /** A2UI v0.9's client function action. */
  functionCall?: FunctionCall
  /** The legacy key the core also reads. */
  function?: FunctionCall
}

/** A data-driven child list: one `component` per item at `path`; `key` = a
 *  pointer relative to the item that identifies it (reordering keeps the
 *  item's state), the index when absent. */
export interface ChildTemplate {
  component: string
  path: string
  key?: string
}

/** The wire form of a template child list (A2UI's `{componentId, path}`,
 *  plus round 1's optional `key`). */
export interface WireChildTemplate {
  componentId: string
  path: string
  key?: string
}

/** The common `visible` value: a literal boolean or a dynamic value the
 *  renderer resolves (truthiness as `expr.ts truthy`: undefined, null, false
 *  and "" hide; 0 shows). Absent = visible. */
export type Visible = boolean | DataBinding | FunctionCall

/** A2UI's AccessibilityAttributes: `label`/`description` are DynamicStrings
 *  (a literal, a binding or a call; the bind pass resolves them). */
export interface AuthoredAccessibility {
  label?: unknown
  description?: unknown
}

/** Round 1 (docs/round-1-contract.md §6): the accessibility a NORMALIZED node
 *  carries: the author's label/description, plus what a macro part's `$a11y`
 *  adds (an author's label/description on a macro win over its root's). Every
 *  value may be dynamic until the bind pass. A painter maps `role` (one of
 *  catalog/a11y.json `roles`) and the states onto the platform: ARIA
 *  role/aria-*, SwiftUI traits/values, Compose semantics, AccessKit. */
export interface NodeAccessibility extends AuthoredAccessibility {
  /** One of `A11Y_ROLES`; absent = the component's own (a11y.json). */
  role?: unknown
  /** Heading level 1..6 (role heading). */
  level?: unknown
  /** A disclosure's state (aria-expanded). */
  expanded?: unknown
  /** `page`, `step` or false (aria-current). */
  current?: unknown
  /** A selected option, tab or row (aria-selected). */
  selected?: unknown
  /** A toggle button's state (aria-pressed). */
  pressed?: unknown
  /** A checkbox/radio state (aria-checked). */
  checked?: unknown
  /** Progressbar/meter/slider values (aria-valuenow/min/max). */
  valueNow?: unknown
  valueMin?: unknown
  valueMax?: unknown
  /** True = out of the accessibility tree (decorative). */
  hidden?: unknown
  /** True = focus lands here when the enclosing dialog opens. */
  autoFocus?: unknown
}

/** A node as it rides A2UI's `updateComponents`: props at the top level,
 *  children by id. The core catalog keeps that convention. */
export interface FlatComponent {
  id: string
  component: string
  children?: string[] | WireChildTemplate
  slots?: Record<string, string>
  on?: Record<string, Action>
  style?: Record<string, unknown>
  visible?: Visible
  accessibility?: AuthoredAccessibility
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
  /** Round 1: falsy once resolved = the node is not rendered and takes no
   *  layout space; the expander sets it for a part whose `$if` was bound. */
  visible?: Visible
  on?: Record<string, Action>
  accessibility?: NodeAccessibility
  children: UiNode[]
  slots?: Record<string, UiNode>
  template?: ChildTemplate
  recipe?: { macro: string; part: string; props: Record<string, unknown> }
}

/** The nested authoring form (fixtures, kitchen sink): the same as UiNode
 *  minus what the reducer fills in. */
export interface NestedNode {
  id: string
  component: string
  props?: Record<string, unknown>
  style?: Record<string, unknown>
  visible?: Visible
  on?: Record<string, Action>
  accessibility?: AuthoredAccessibility
  children?: NestedNode[]
  slots?: Record<string, NestedNode>
  template?: ChildTemplate
}

export interface ReduceIssue {
  id: string
  message: string
}

/** Round 1 (docs/round-1-contract.md §4–6): what a host sets per surface.
 *  Every field is optional; the defaults are what a renderer assumes. */
export interface SurfaceSettings {
  /** BCP 47; drives the format functions, calendar names, the week start
   *  and the text direction (catalog/locale.json). Default `en-US`. */
  locale?: string
  /** Round 2: the IANA zone instants format in (the host Formatter's;
   *  default the platform's; the English fallback and the fixtures: UTC). */
  timeZone?: string
  /** Built-in UI string overrides by id (catalog/strings.json). */
  strings?: Record<string, string>
  /** `system` (default) = the platform's light/dark preference. */
  mode?: `light` | `dark` | `system`
  /** Scales `$control.*` and `$spacing.*` by the theme's `$density.*`. */
  density?: `compact` | `default` | `comfortable`
  /** `system` (default) = the platform's high-contrast setting; `high`
   *  forces the theme's `contrast` overlays. */
  contrast?: `normal` | `high` | `system`
  /** The theme id (a built-in or one the host loaded). */
  theme?: string
}

/** Round 1 (catalog/a11y.json `commands`): what a host may ask of a live
 *  surface. */
export type SurfaceCommand =
  | { focus: { id: string } }
  | { announce: { text: string; live?: `polite` | `assertive` } }
  | { scrollIntoView: { id: string } }
  /** Round 2: bring item `index` (data order) of List/Table `id` into view. */
  | { scrollToIndex: { id: string; index: number; align?: `start` | `center` | `end` | `nearest` } }
