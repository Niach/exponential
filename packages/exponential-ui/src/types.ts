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
  /** Round 9 §5.3: a component id reference (Label.for, Popover.anchor, Menu.anchor); a string. */
  | `componentId`
  /** Round 9 §6.6: a binding or a call ONLY, never a literal (a check's `condition`). */
  | `dynamic`

export interface PropSchema {
  type: PropType
  /** Model-facing. VAPP-106: ABSENT at runtime on the runtime catalog (`coreCatalog`) until
   *  `describeCatalog()` attaches it; typed present because its readers (the prompt, the schema
   *  emitter, the docs, an extension's validation) always see it attached. */
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
  /** `number`: the inclusive bounds a literal must respect (R8 F51: a
   *  Rating's `max` stops at 100); both validators refuse a value outside. */
  minimum?: number
  maximum?: number
  /** `array`: the item schema. */
  items?: PropSchema
  /** `object`: the name of a shared shape in `defs`; none = any object. */
  shape?: string
  /** VAPP-217: FILLED BY THE HOST (the record macros' fields, columns,
   *  bindings), never written by an author: the model-facing prompt leaves
   *  it out. */
  host?: boolean
  /** Round 9 §11: `deprecated` = the 1.0 schema marks the prop `deprecated: true`
   *  with `x-deprecated-reason` (`deprecatedReason`, else the description). */
  status?: `deprecated`
  deprecatedReason?: string
}

export interface DefSchema {
  /** Model-facing. VAPP-106: ABSENT at runtime on the runtime catalog (`coreCatalog`) until
   *  `describeCatalog()` attaches it; typed present because its readers (the prompt, the schema
   *  emitter, the docs, an extension's validation) always see it attached. */
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
  /** `planned` = in the catalog, painters land later. */
  status?: string
  /** Round 9 §5.6: the component types it may sit in (`Surface` = as the
   *  surface root); the 1.0 `allowedParents`. Absent = anywhere. */
  parents?: readonly string[]
  /** Model-facing. VAPP-106: ABSENT at runtime on the runtime catalog (`coreCatalog`) until
   *  `describeCatalog()` attaches it; typed present because its readers (the prompt, the schema
   *  emitter, the docs, an extension's validation) always see it attached. */
  description: string
  props: Record<string, PropSchema>
  events?: readonly string[]
  /** The docs' and the fixtures' specimen props; absent on the runtime catalog like `description`. */
  example?: Record<string, unknown>
}

export interface CatalogSource {
  id: string
  liteId: string
  name: string
  version: string
  /** Round 9 §11: the catalog rules (Markdown), the 1.0 catalog file's
   *  `instructions`; model-facing, so it lives on the descriptions side. */
  instructions?: string
  unknownComponent: string
  enums: Record<string, readonly string[]>
  defs: Record<string, DefSchema>
  functions: { names: readonly string[]; core: Record<string, CoreFunctionDef>; basic: Record<string, FunctionSignature> }
  /** Round 1: `<Component>.<part>[.<variant>]` → the icons.json name a
   *  renderer draws for a part it owns (`$comment` aside). */
  builtinIcons: Record<string, string>
  components: Record<string, ComponentDef>
}

/** Round 4: a basic function's signature (core.catalog.json `functions.basic`). */
export interface FunctionSignature {
  /** Argument name → its type (`number`, `string`, `boolean`, `array`, `object`, `any`). */
  args: Record<string, string>
  /** `string number boolean array object any void`, or (round 9) `validationResult`. */
  returns: string
}

/** Round 1: one core function (core.catalog.json `functions.core`). */
export interface CoreFunctionDef {
  /** Model-facing. VAPP-106: ABSENT at runtime on the runtime catalog (`coreCatalog`) until
   *  `describeCatalog()` attaches it; typed present because its readers (the prompt, the schema
   *  emitter, the docs, an extension's validation) always see it attached. */
  description: string
  /** Argument name → its type (`number`, `string`, `boolean`, `array`, `object`, `any`). */
  args: Record<string, string>
  returns: string
}

/** A child entry of a macro template: a part, the author's children
 *  (`$children`) or one of the author's slots spliced in place (`$slot:name`). */
export type MacroChild = MacroTemplate | MacroSlotSplice | `$children` | `$slot:${string}`

/** #VAPP-277: an author slot spliced in place only while `$if`/`$any` hold
 *  (a bound condition splices it with `visible`). */
export interface MacroSlotSplice {
  $slot: string
  $if?: unknown
  $any?: unknown[]
}

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
  /** An expression over the props, or (EXP-1760) a literal list (a picker's default set). */
  $each?: string | unknown[]
  $as?: string
  /** Part event → the author's macro event (`$context` merged into it), or
   *  #VAPP-284 `{event, context}` = that event with its OWN context (a part
   *  routing two events that must not share one). */
  $on?: Record<string, string | MacroRoute>
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
  /** eApps U3 (VAPP-232): a DATA TEMPLATE part: the part renders `item`
   *  once per entry of the array at `path` (a data pointer, evaluated like
   *  any value; absolute, or relative inside an outer template), keyed by
   *  `key`. The item node (id `<part id>.<item part>`) is lifted into the
   *  reducer's `templates`; it reads its entry through relative bindings. */
  $template?: { path: unknown; key?: unknown; item: MacroTemplate }
}

/** #VAPP-277: a macro's PASS DOWN, applied BEFORE its slot expands (macros
 *  expand bottom-up, so a template cannot restyle its slot's nodes): when
 *  `$if` holds (evaluated in the macro's own context, literal only), every
 *  `component` node in the author's `slot` subtree (a nested node of the same
 *  macro ends the walk) takes `props` (evaluated like any template value, so a
 *  bound macro prop passes the binding itself), never over a prop its author set. */
export interface MacroPass {
  slot: string
  component: string
  $if?: unknown
  props: Record<string, unknown>
}

/** #VAPP-284: a `$on` entry with its own context. */
export interface MacroRoute {
  event: string
  context?: Record<string, unknown>
}

export interface MacroDef {
  recipeProps: readonly string[]
  root: MacroTemplate
  $pass?: readonly MacroPass[]
  /** #VAPP-284: other roots, tried in order: the first whose `$if` holds
   *  (decided at expansion: a bound condition never picks one) replaces
   *  `root`, and its root part keeps the author's id whatever it is named.
   *  The conditions also see `{childCount}` (the author's children). */
  alternatives?: readonly { $if: unknown; root: MacroTemplate }[]
  /** #EXP-1760: parts of the ROOT native the macro styles too (an
   *  IconPicker's Popover `content`): `<Macro>/<part>` layers over
   *  `<Native>/<part>` on every renderer. */
  rootParts?: readonly string[]
}

/** An extension catalog: its own id, extending the core, with components and
 *  the macro templates for its macro components. */
export interface ExtensionDef {
  id: string
  name: string
  /** VAPP-161: `MAJOR.MINOR.PATCH`, what a package's `extensions` entry
   *  is checked against (caret); unset = satisfies any. */
  version?: string
  extends: string
  /** Round 9 §11: the extension's own rules (Markdown) for its 1.0 catalog file. */
  instructions?: string
  enums?: Record<string, readonly string[]>
  defs?: Record<string, DefSchema>
  components: Record<string, ComponentDef>
  macros?: Record<string, MacroDef>
}

/** VAPP-289 (round 9 §3): an A2UI 1.0 data binding and function call. */
export type { DataBinding, FunctionCall } from "./expr"
import type { DataBinding, FunctionCall } from "./expr"
/** An A2UI 1.0 Action (our core `on` form, §5.3 `EuiAction`): a server event,
 *  a renderer function, or BOTH (round 1: evaluate the function args and the
 *  event context first, then run the function, then dispatch the event). */
export interface Action {
  event?: { name: string; context?: Record<string, unknown>; userMessage?: unknown }
  /** The call itself (`{"@call", args?, catalogId?}`). */
  functionCall?: FunctionCall
}

/** VAPP-289 (round 9 §5.1): `metadata` of a component, a surface, an action. */
export interface Metadata {
  extensions?: Record<string, unknown>
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
  /** Round 9 §5.1: `off` (default) | `polite` | `assertive`. */
  live?: unknown
  /** Round 9 §5.1: a DynamicBoolean; true = out of the accessibility tree (still painted). */
  hidden?: unknown
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
  /** Round 9 §5.5: overrides the surface's default catalog. */
  catalogId?: string
  /** Round 9 §5.1: stored and exposed, never painted. */
  metadata?: Metadata
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
  /** Round 9 §5.1: the authored component's `metadata` (never painted). */
  metadata?: Metadata
  /** `authored` (VAPP-163, expansion roots only): the top-level style keys
   *  written ON the macro node, sorted; they win over the part recipe. */
  recipe?: { macro: string; part: string; props: Record<string, unknown>; authored?: string[] }
}

/** The nested authoring form (fixtures, kitchen sink): the same as UiNode
 *  minus what the reducer fills in. */
export interface NestedNode {
  id: string
  component: string
  /** Round 9 §13: only the core id. */
  catalogId?: string
  metadata?: Metadata
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
  /** Round 9 §5.2: the error code the runtime SENDS (VALIDATION_FAILED,
   *  UNALLOWED_CHILD, UNALLOWED_PARENT, UNSUPPORTED_CATALOG). */
  code?: string
  /** `/components/<authored id>` + `/<prop pointer>`. */
  path?: string
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
  /** VAPP-200: the viewer may read, not write (an app at access `read`):
   *  every writing control paints disabled (`readOnlyTree`). Default off. */
  readOnly?: boolean
}

/** Round 1 (catalog/a11y.json `commands`): what a host may ask of a live
 *  surface. */
export type SurfaceCommand =
  | { focus: { id: string } }
  | { announce: { text: string; live?: `polite` | `assertive` } }
  | { scrollIntoView: { id: string } }
  /** Round 2: bring item `index` (data order) of List/Table `id` into view. */
  | { scrollToIndex: { id: string; index: number; align?: `start` | `center` | `end` | `nearest` } }
