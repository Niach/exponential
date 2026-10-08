// VAPP-85: the model-facing catalog prompt. One compact block: the catalog
// id, the authoring rules, one line per component (description, then props
// with type and one-sentence meaning), the functions, then any registered
// extensions in the same shape. fixtures/prompt-budget.json records the size
// of the full core prompt; prompt.test.ts fails when it grows past the
// budget — cut descriptions before cutting components.

import { CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, catalogView, coreCatalog } from "./catalog"
import type { ComponentDef, ExtensionDef, PropSchema } from "./types"

export interface PromptOptions {
  extensions?: readonly ExtensionDef[]
  /** The core-lite subset (no overlays, media or Chart). */
  lite?: boolean
  /** Omit the per-prop lines (names + types only). */
  terse?: boolean
}

/** The rules every surface author (a model) must follow, shared by the core
 *  and the basic catalog. Kept to a few lines on purpose. */
export const PROMPT_RULES: readonly string[] = [
  `Reply with A2UI v0.9 messages: createSurface{surfaceId, catalogId}, then updateComponents{surfaceId, components:[…]}; one component has id "root".`,
  `A component = {id, component, …props, children?: [ids] | {componentId, path, key?}, slots?: {name: id}, on?: {event: action}, style?, visible?}. Children are ids, never inline; {componentId, path} repeats per item, key = the item field naming it.`,
  `Include every required prop. A ~ prop may be a binding {path: "/json/pointer"} or a call {call, args}; bound values stay live. visible: false (or a binding) hides any component.`,
  `Lay out with Stack, Grid, Card, Sidebar and List (anything longer than a screen); Box + style only for the rest. Wrap fields in Form.`,
  `Responsive: a ^ prop also takes {base, sm?, md?, lg?, xl?} (from that breakpoint up). Style conditions, one level, narrow first: "@media (min-width: $breakpoint.md)": {…} (also max-width, min-height, max-height in px or $breakpoint.sm|md|lg|xl; orientation: portrait|landscape; hover: hover|none; prefers-reduced-motion: reduce), then ":hover", ":focus-visible", ":pressed". display: "none" hides.`,
  `Prefer catalog components over Markdown, labels and tones over colours. Built-in copy (Cancel, Search…) is localized: leave it unset.`,
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

function componentBlock(name: string, def: ComponentDef, enums: Record<string, readonly string[]>, terse: boolean): string {
  const head = `${name}${def.kind === `macro` ? `` : ``}: ${def.description}`
  const meta: string[] = []
  if (def.children !== `none`) meta.push(`children: ${def.children}`)
  if (def.slots?.length) meta.push(`slots: ${def.slots.join(`, `)}`)
  if (def.events?.length) meta.push(`events: ${def.events.join(`, `)}`)
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

/** The system prompt for a surface author. */
export function catalogPrompt(options: PromptOptions = {}): string {
  const extensions = options.extensions ?? []
  const view = catalogView(extensions)
  const lines: string[] = []
  const id = options.lite ? CORE_LITE_CATALOG_ID : CORE_CATALOG_ID
  lines.push(`Catalog ${id}${extensions.length ? ` with extensions ${extensions.map((e) => e.id).join(`, `)}` : ``}.`)
  lines.push(`Rules:`)
  for (const rule of PROMPT_RULES) lines.push(`- ${rule}`)
  lines.push(`Components (props marked * are required, ~ accept a binding, ^ are responsive; =x is the default, else an enum defaults to its first value and a boolean to false):`)
  if (!options.terse) lines.push(`Common props (no description below): ${Object.entries(COMMON_PROPS).map(([k, v]) => (v ? `${k} (${v})` : k)).join(`, `)}.`)
  for (const [name, def] of Object.entries(coreCatalog.components)) {
    if (def.hidden) continue
    if (options.lite && !def.lite) continue
    lines.push(componentBlock(name, def, view.enums, options.terse ?? false))
  }
  const listed = Object.values(coreCatalog.components).filter((def) => !def.hidden && (!options.lite || def.lite))
  lines.push(`Shapes: ${defsBlock(coreCatalog.defs, view.enums, usedShapes(listed, coreCatalog.defs)).join(`; `)}`)
  lines.push(`Functions (client-side, the A2UI basic ones plus core ones): ${coreCatalog.functions.names.join(`, `)}.`)
  lines.push(`Style keys (Box): display, flex*, justifyContent, align*, gap, width, height, min/max sizes, aspectRatio, position, top/right/bottom/left, inset*, padding*/margin* (+Horizontal/Vertical/InlineStart/End), gridTemplateColumns/Rows/Areas, gridArea, overflow(X/Y), backgroundColor, backgroundGradient {angle, stops}, color, border(Top…)Width, borderColor, borderStyle, borderRadius (+corners), opacity, boxShadow, font*, letterSpacing, textAlign, textDecoration, textTransform, transition $motion.*, transform (translate/scale/rotate, paint only), visibility, pointerEvents, userSelect, cursor. Values: px, "N%", "auto", or tokens $spacing.md, $color.primary, $radius.lg, $control.row.`)
  for (const ext of extensions) {
    lines.push(`Extension ${ext.id} (${ext.name}):`)
    for (const [name, def] of Object.entries(ext.components)) lines.push(componentBlock(name, def, view.enums, options.terse ?? false))
    if (ext.defs && Object.keys(ext.defs).length) lines.push(`Shapes: ${defsBlock(ext.defs, view.enums).join(`; `)}`)
  }
  return lines.join(`\n`)
}

/** Characters per token the budget assumes (the same rule as the context
 *  layout's `charsPerToken`): an estimate, not a tokenizer. */
export const CHARS_PER_TOKEN = 4

export function estimateTokens(text: string): number {
  return Math.ceil(text.length / CHARS_PER_TOKEN)
}
