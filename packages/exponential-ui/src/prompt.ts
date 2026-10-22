// VAPP-85: the model-facing catalog prompt. One compact block: the catalog
// id, the authoring rules, one line per component (description, then props
// with type and one-sentence meaning), the functions, then any registered
// extensions in the same shape. fixtures/prompt-budget.json records the size
// of the full core prompt; prompt.test.ts fails when it grows past the
// budget — cut descriptions before cutting components.
// VAPP-108: the prompt is SECTIONED (`sections`): a host serves the rules +
// the index first and `componentPrompt(name)` on demand, or `only` the
// components a task needs; eval/ measures what each variant costs.

import { CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, catalogView, coreCatalog, isOffered } from "./catalog"
import type { ComponentDef, ExtensionDef, PropSchema } from "./types"

/** The sections of the prompt, in print order (VAPP-108): an agent (or the
 *  host serving it) asks for the ones it needs. `index` = one line per
 *  component (name, group, description) instead of the full `components`
 *  blocks: the agent reads the index, then asks for `componentPrompt(name)`
 *  of the ones it will use. */
export type PromptSection = `rules` | `components` | `index` | `shapes` | `functions` | `style` | `extensions`
export const PROMPT_SECTIONS: readonly PromptSection[] = [`rules`, `components`, `index`, `shapes`, `functions`, `style`, `extensions`]
/** What `catalogPrompt()` prints without `sections`. */
export const DEFAULT_SECTIONS: readonly PromptSection[] = [`rules`, `components`, `shapes`, `functions`, `style`, `extensions`]

export interface PromptOptions {
  extensions?: readonly ExtensionDef[]
  /** The core-lite subset (the app-shaped vocabulary: no overlays, media, Chart, data tables or the rarer controls). */
  lite?: boolean
  /** Omit the per-prop lines (names + types only). */
  terse?: boolean
  /** VAPP-108: which sections to print (default `DEFAULT_SECTIONS`, in that order whatever the order given). */
  sections?: readonly PromptSection[]
  /** VAPP-108: print only these components (full blocks) and the shapes they reach; `index` still lists every one. */
  only?: readonly string[]
}

/** The rules every surface author (a model) must follow, shared by the core
 *  and the basic catalog. Kept to a few lines on purpose. */
export const PROMPT_RULES: readonly string[] = [
  `Reply with A2UI v0.9 messages: createSurface{surfaceId, catalogId}, then updateComponents{surfaceId, components:[…]} (one has id "root"; resend an id to change it) and updateDataModel{surfaceId, path, value} (writes the JSON pointer; no value removes it). Keep data in the model and bind to it.`,
  `A component = {id, component, …props, children?: [ids] | {componentId, path, key?}, slots?: {name: id}, on?: {event: action}, style?, visible?}. Children are ids, never inline; {componentId, path} repeats per item, key = the item field naming it.`,
  `Include every required prop. A ~ prop may be a binding {path: "/json/pointer"} or a call {call, args}; bound values stay live. visible: false (or a binding) hides any component.`,
  `An action (on: {event: action}) = {event: {name, context?}} (sent to you; context values may be bindings, read AFTER the component's own write) and/or {functionCall: {call, args}} (runs on the client: a function below, or a namespaced host function like app.toast).`,
  `Lay out with Stack, Grid, Card, Sidebar and List (anything longer than a screen); Box + style only for the rest. Wrap fields in Form; a valid submit closes the Dialog or Drawer around it.`,
  `Responsive: a ^ prop also takes {base, sm?, md?, lg?, xl?} (from that breakpoint up). Style conditions, one level, narrow first: "@media (min-width: $breakpoint.md)": {…} (also max-width, min-height, max-height in px or $breakpoint.sm|md|lg|xl; orientation: portrait|landscape; hover: hover|none; prefers-reduced-motion: reduce), then ":hover", ":focus-visible", ":pressed". display: "none" hides.`,
  `Prefer catalog components over Markdown, labels and tones over colours; a colour is a $color.* token, never #hex (light and dark both apply). Built-in copy (Cancel, Search…) is localized: leave it unset.`,
]

/** Props whose meaning is the same everywhere: described ONCE in the full
 *  prompt (the per-prop line then carries only name, marks and type). The
 *  catalog keeps every per-component description for the docs. */
export const COMMON_PROPS: Readonly<Record<string, string>> = {
  name: `form key`,
  label: ``,
  placeholder: ``,
  disabled: ``,
  checks: `validation; every failure shows`,
  validateOn: ``,
  style: `extra Box style`,
  icon: ``,
  size: ``,
  tone: ``,
  open: `bind it`,
  side: ``,
  emptyText: ``,
  dismissible: ``,
  busy: ``,
  loading: ``,
  lines: `truncate`,
  width: `px`,
  height: `px`,
  value: `bind it for two-way state`,
  title: ``,
  subtitle: ``,
  description: ``,
  meta: ``,
  count: ``,
  min: ``,
  max: ``,
  step: ``,
  src: ``,
  alt: ``,
  multiple: ``,
  pressable: `fires press`,
  durationMs: `known length`,
  firstDayOfWeek: `0 = Sunday; else the locale's`,
  direction: ``,
  orientation: ``,
  gap: ``,
  aspectRatio: `width/height`,
  text: ``,
}

function typeOf(schema: PropSchema, enums: Record<string, readonly string[]>): string {
  switch (schema.type) {
    case `enum`:
      return (schema.values ?? enums[schema.enum ?? ``] ?? []).join(`|`)
    case `array`:
      return schema.items ? `[${schema.items.type === `object` ? (schema.items.shape ?? `object`) : typeOf(schema.items, enums)}]` : `[]`
    case `object`:
      return schema.shape ?? `object`
    default:
      return schema.type
  }
}

function marksOf(schema: PropSchema): string {
  return `${schema.required ? `*` : ``}${schema.bindable ? `~` : ``}${schema.responsive ? `^` : ``}`
}

/** The default worth printing: an enum's FIRST value and a boolean's
 *  `false` are implied (the components line says so once). */
function defaultOf(schema: PropSchema, enums: Record<string, readonly string[]>): string {
  if (schema.default === undefined) return ``
  if (schema.type === `boolean` && schema.default === false) return ``
  if (schema.type === `enum` && (schema.values ?? enums[schema.enum ?? ``] ?? [])[0] === schema.default) return ``
  return `=${JSON.stringify(schema.default)}`
}

function propLine(name: string, schema: PropSchema, enums: Record<string, readonly string[]>): string {
  const def = defaultOf(schema, enums)
  const head = `${name}${marksOf(schema)}: ${typeOf(schema, enums)}${def}`
  return name in COMMON_PROPS ? head : `${head} — ${schema.description}`
}

export function componentBlock(name: string, def: ComponentDef, enums: Record<string, readonly string[]>, terse: boolean): string {
  const head = `${name}${def.kind === `macro` ? `` : ``}: ${def.description}`
  const meta: string[] = []
  if (def.children !== `none`) meta.push(def.children === `one` ? `one child` : `children`)
  if (def.slots?.length) meta.push(`slots: ${def.slots.join(`, `)}`)
  if (def.events?.length) meta.push(`on: ${def.events.join(`, `)}`)
  const props = Object.entries(def.props)
  const body = terse
    ? [`  props: ${props.map(([p, s]) => `${p}${marksOf(s)}:${typeOf(s, enums)}`).join(`, `)}`]
    : props.map(([p, s]) => `  ${propLine(p, s, enums)}`)
  return [head, ...(meta.length ? [`  ${meta.join(`; `)}`] : []), ...body].join(`\n`)
}

/** The shapes a prompt needs: every def a listed component's props reach
 *  (directly, through arrays or through another def). */
function usedShapes(components: Iterable<ComponentDef>, defs: Record<string, { properties: Record<string, PropSchema> }>): Set<string> {
  const used = new Set<string>()
  const visit = (schema: PropSchema | undefined) => {
    if (!schema) return
    if (schema.type === `array`) return visit(schema.items)
    if (schema.type === `object` && schema.shape && !used.has(schema.shape) && defs[schema.shape]) {
      used.add(schema.shape)
      for (const p of Object.values(defs[schema.shape].properties)) visit(p)
    }
  }
  for (const def of components) for (const p of Object.values(def.props)) visit(p)
  return used
}

function defsBlock(defs: Record<string, { description: string; properties: Record<string, PropSchema> }>, enums: Record<string, readonly string[]>, only?: Set<string>): string[] {
  return Object.entries(defs).filter(([name]) => !only || only.has(name)).map(
    ([name, def]) => `${name} = {${Object.entries(def.properties).map(([p, s]) => `${p}${s.required ? `*` : ``}: ${typeOf(s, enums)}`).join(`, `)}}`
  )
}

const SHORT_TYPES: Readonly<Record<string, string>> = { string: `str`, number: `num`, boolean: `bool`, object: `obj` }

/** Every catalog function as `name(arg:type,…)→returns` (round 4: the
 *  model sees the argument names, not just the function names). */
export function functionSignatures(): string[] {
  const { names, core, basic } = coreCatalog.functions
  return names.map((name) => {
    const sig = core[name] ?? basic[name]
    if (!sig) return name
    return `${name}(${Object.entries(sig.args).map(([a, t]) => `${a}:${SHORT_TYPES[t] ?? t}`).join(`,`)})→${SHORT_TYPES[sig.returns] ?? sig.returns}`
  })
}

/** VAPP-108: one component's full block on demand (the per-component
 *  section an agent requests after reading the index), with the shapes its
 *  props reach. Unknown or hidden names give an empty string. */
export function componentPrompt(name: string, options: Pick<PromptOptions, `extensions` | `terse`> = {}): string {
  const extensions = options.extensions ?? []
  const view = catalogView(extensions)
  const def = view.components[name]
  if (!def || def.hidden) return ``
  const defs = { ...coreCatalog.defs, ...Object.fromEntries(extensions.flatMap((e) => Object.entries(e.defs ?? {}))) }
  const lines = [componentBlock(name, def, view.enums, options.terse ?? false)]
  const shapes = defsBlock(defs, view.enums, usedShapes([def], defs))
  if (shapes.length) lines.push(`Shapes: ${shapes.join(`; `)}`)
  return lines.join(`\n`)
}

/** VAPP-108: the index — every offered component on one line (name, group, kind, its sentence), grouped. */
export function catalogIndex(options: Pick<PromptOptions, `extensions` | `lite`> = {}): string[] {
  const groups = new Map<string, string[]>()
  const add = (name: string, def: ComponentDef) => {
    const list = groups.get(def.group) ?? []
    list.push(`${name}${def.children === `none` ? `` : def.children === `one` ? ` (one child)` : ` (children)`}: ${def.description}`)
    groups.set(def.group, list)
  }
  for (const [name, def] of Object.entries(coreCatalog.components)) {
    if (!isOffered(def)) continue
    if (options.lite && !def.lite) continue
    add(name, def)
  }
  for (const ext of options.extensions ?? []) for (const [name, def] of Object.entries(ext.components)) add(name, def)
  return [...groups.entries()].map(([group, items]) => `${group}: ${items.join(` · `)}`)
}

/** The system prompt for a surface author. */
export function catalogPrompt(options: PromptOptions = {}): string {
  const extensions = options.extensions ?? []
  const view = catalogView(extensions)
  const wanted = new Set(options.sections ?? DEFAULT_SECTIONS)
  const has = (s: PromptSection) => wanted.has(s)
  const only = options.only ? new Set(options.only) : undefined
  const lines: string[] = []
  const id = options.lite ? CORE_LITE_CATALOG_ID : CORE_CATALOG_ID
  lines.push(`Catalog ${id}${extensions.length ? ` with extensions ${extensions.map((e) => e.id).join(`, `)}` : ``}.`)
  if (has(`rules`)) {
    lines.push(`Rules:`)
    for (const rule of PROMPT_RULES) lines.push(`- ${rule}`)
  }
  const offered = Object.entries(coreCatalog.components).filter(([, def]) => isOffered(def) && (!options.lite || def.lite))
  const listed = offered.filter(([name]) => !only || only.has(name))
  if (has(`components`)) {
    lines.push(`Components (props marked * are required, ~ accept a binding, ^ are responsive; =x is the default, else an enum defaults to its first value and a boolean to false${only ? `; ask for any other component by name` : ``}):`)
    if (!options.terse) lines.push(`Common props (no description below): ${Object.entries(COMMON_PROPS).map(([k, v]) => (v ? `${k} (${v})` : k)).join(`, `)}.`)
    for (const [name, def] of listed) lines.push(componentBlock(name, def, view.enums, options.terse ?? false))
  }
  if (has(`index`)) {
    lines.push(`Index (every component; ask for one by name to get its props):`)
    for (const line of catalogIndex({ extensions, lite: options.lite })) lines.push(`- ${line}`)
  }
  if (has(`shapes`) && has(`components`)) lines.push(`Shapes: ${defsBlock(coreCatalog.defs, view.enums, usedShapes(listed.map(([, def]) => def), coreCatalog.defs)).join(`; `)}`)
  if (has(`functions`)) lines.push(`Functions ({call, args}; name(arg:type)→result): ${functionSignatures().join(`, `)}. filter: where = {field: value} (an empty value matches all), query = a case-insensitive substring of fields; bind a List or Table to it.`)
  if (has(`style`)) lines.push(`Style keys (Box): display, flex*, justifyContent, align*, gap, width, height, min/max sizes, aspectRatio, position (relative|absolute|sticky), top/right/bottom/left, inset*, padding*/margin* (+Horizontal/Vertical/InlineStart/End), grid*, overflow(X/Y), direction, backgroundColor, backgroundGradient {angle, stops}, backdropBlur $blur.*, color, border*, opacity, boxShadow, font*, letterSpacing, text*, transition $motion.*, transform (translate/scale/rotate, paint only), animation (pulse|spin|fade-in|slide-in-up/down/left/right|shimmer), visibility, pointerEvents, userSelect, cursor. Values: px, "N%", "auto", or tokens $spacing.md, $color.primary, $radius.lg, $control.row.`)
  if (has(`extensions`) && has(`components`)) {
    for (const ext of extensions) {
      lines.push(`Extension ${ext.id} (${ext.name}):`)
      for (const [name, def] of Object.entries(ext.components)) lines.push(componentBlock(name, def, view.enums, options.terse ?? false))
      if (ext.defs && Object.keys(ext.defs).length) lines.push(`Shapes: ${defsBlock(ext.defs, view.enums).join(`; `)}`)
    }
  }
  return lines.join(`\n`)
}

/** Characters per token the budget assumes (the same rule as the context
 *  layout's `charsPerToken`): an estimate, not a tokenizer. */
export const CHARS_PER_TOKEN = 4

export function estimateTokens(text: string): number {
  return Math.ceil(text.length / CHARS_PER_TOKEN)
}
