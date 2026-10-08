// VAPP-85: the model-facing catalog prompt. One compact block: the catalog
// id, the authoring rules, one line per component (description, then props
// with type and one-sentence meaning), the functions, then any registered
// extensions in the same shape. fixtures/prompt-budget.json records the size
// of the full core prompt; prompt.test.ts fails when it grows past the
// budget — cut descriptions before cutting components.

import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, catalogView, coreCatalog } from "./catalog"
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
  `A component = {id, component, …props, children?: [ids] | {componentId, path}, slots?: {name: id}, on?: {event: action}, style?}. Children are ids, never inline.`,
  `Include every required prop. A prop marked ~ may be a data binding {path: "/json/pointer"} or a function call {call, args}.`,
  `Lay out with Stack, Grid, Card and List; use Box with a style object only for what those cannot do. Lists longer than a screen use List.`,
  `Use the catalog's components over markdown; Markdown only for prose. Prefer labels and tones over colours.`,
]

function typeOf(schema: PropSchema, enums: Record<string, readonly string[]>): string {
  switch (schema.type) {
    case `enum`:
      return (schema.values ?? enums[schema.enum ?? ``] ?? []).join(`|`)
    case `array`:
      return schema.items ? `[${schema.items.type === `object` ? schema.items.shape : typeOf(schema.items, enums)}]` : `[]`
    case `object`:
      return schema.shape ?? `object`
    default:
      return schema.type
  }
}

function propLine(name: string, schema: PropSchema, enums: Record<string, readonly string[]>): string {
  const marks = `${schema.required ? `*` : ``}${schema.bindable ? `~` : ``}`
  const def = schema.default !== undefined ? `=${JSON.stringify(schema.default)}` : ``
  return `${name}${marks}: ${typeOf(schema, enums)}${def} — ${schema.description}`
}

function componentBlock(name: string, def: ComponentDef, enums: Record<string, readonly string[]>, terse: boolean): string {
  const head = `${name}${def.kind === `macro` ? `` : ``}: ${def.description}`
  const meta: string[] = []
  if (def.children !== `none`) meta.push(`children: ${def.children}`)
  if (def.slots?.length) meta.push(`slots: ${def.slots.join(`, `)}`)
  if (def.events?.length) meta.push(`events: ${def.events.join(`, `)}`)
  const props = Object.entries(def.props)
  const body = terse
    ? [`  props: ${props.map(([p, s]) => `${p}${s.required ? `*` : ``}:${typeOf(s, enums)}`).join(`, `)}`]
    : props.map(([p, s]) => `  ${propLine(p, s, enums)}`)
  return [head, ...(meta.length ? [`  ${meta.join(`; `)}`] : []), ...body].join(`\n`)
}

function defsBlock(defs: Record<string, { description: string; properties: Record<string, PropSchema> }>, enums: Record<string, readonly string[]>): string[] {
  return Object.entries(defs).map(
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
  lines.push(`Components (props marked * are required, ~ accept a binding):`)
  for (const [name, def] of Object.entries(coreCatalog.components)) {
    if (def.hidden) continue
    if (options.lite && !def.lite) continue
    lines.push(componentBlock(name, def, view.enums, options.terse ?? false))
  }
  lines.push(`Shapes: ${defsBlock(coreCatalog.defs, view.enums).join(`; `)}`)
  lines.push(`Functions (client-side, same as the A2UI basic catalog ${A2UI_BASIC_CATALOG_ID}): ${coreCatalog.functions.names.join(`, `)}.`)
  lines.push(`Style keys (Box): display, flexDirection, flexWrap, justifyContent, alignItems, alignSelf, flexGrow, flexShrink, flexBasis, gap, width, height, min/max sizes, aspectRatio, position, top/right/bottom/left, padding*/margin* (+InlineStart/End), gridTemplateColumns/Rows/Areas, gridArea, overflow, backgroundColor, color, borderWidth, borderColor, borderRadius, opacity, boxShadow, fontSize, fontWeight, lineHeight, textAlign. Values: px numbers, "N%", "auto", or tokens $spacing.md, $color.primary, $radius.lg, $control.row.`)
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
