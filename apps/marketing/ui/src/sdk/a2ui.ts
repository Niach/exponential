/* Pure A2UI helpers of the site (no renderer, no React): the nested
   authoring form → the flat A2UI v0.9 `updateComponents` list, the message
   envelope, the prop-control model the component pages generate from the
   docs, and the playground's input parser. Imported by page chunks, so it
   stays free of the SDK packages (the renderer loads in the islands). */

export const A2UI_VERSION = `v0.9`
export const CORE_CATALOG = `https://ui.exponential.at/catalogs/core/v1`

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }

/** The nested authoring form (fixtures/specimens.json). */
export interface Nested {
  id: string
  component: string
  props?: Record<string, unknown>
  style?: Record<string, unknown>
  on?: Record<string, unknown>
  accessibility?: Record<string, unknown>
  children?: Nested[]
  slots?: Record<string, Nested>
  template?: { component: string; path: string }
}

/** One component as it rides `updateComponents`: props at the top level,
 *  children and slots by id. */
export interface Flat {
  id: string
  component: string
  children?: string[] | { componentId: string; path: string }
  slots?: Record<string, string>
  [prop: string]: unknown
}

export interface Message {
  version?: string
  createSurface?: { surfaceId: string; catalogId: string; theme?: unknown; sendDataModel?: boolean }
  updateComponents?: { surfaceId: string; components: Flat[] }
  updateDataModel?: { surfaceId: string; path?: string; value?: unknown }
  deleteSurface?: { surfaceId: string }
  [key: string]: unknown
}

export const MESSAGE_KINDS = [`createSurface`, `updateComponents`, `updateDataModel`, `deleteSurface`] as const

/** The nested form → flat components, pre-order; the top node becomes
 *  `rootId` (A2UI: one component has id "root"). */
export function flatten(tree: Nested, rootId = `root`): Flat[] {
  const out: Flat[] = []
  const walk = (n: Nested, id: string) => {
    const flat: Flat = { id, component: n.component, ...(n.props ?? {}) }
    if (n.template) flat.children = { componentId: n.template.component, path: n.template.path }
    else if (n.children?.length) flat.children = n.children.map((c) => c.id)
    if (n.slots && Object.keys(n.slots).length) flat.slots = Object.fromEntries(Object.entries(n.slots).map(([k, v]) => [k, v.id]))
    if (n.on) flat.on = n.on
    if (n.style) flat.style = n.style
    if (n.accessibility) flat.accessibility = n.accessibility
    out.push(flat)
    if (!n.template) for (const c of n.children ?? []) walk(c, c.id)
    for (const s of Object.values(n.slots ?? {})) walk(s, s.id)
  }
  walk(tree, rootId)
  return out
}

export const createSurface = (surfaceId: string, catalogId = CORE_CATALOG): Message => ({ version: A2UI_VERSION, createSurface: { surfaceId, catalogId } })
export const updateComponents = (surfaceId: string, components: Flat[]): Message => ({ version: A2UI_VERSION, updateComponents: { surfaceId, components } })

/** A complete surface as messages: createSurface + one updateComponents. */
export const surfaceMessages = (surfaceId: string, components: Flat[], catalogId = CORE_CATALOG): Message[] => [createSurface(surfaceId, catalogId), updateComponents(surfaceId, components)]

/** A streamed replay: createSurface, then updateComponents in growing
 *  chunks (each chunk adds the next `step` components; a component whose
 *  children are not there yet renders them as they land). */
export function replayChunks(surfaceId: string, components: Flat[], step = 2, catalogId = CORE_CATALOG): Message[] {
  const out: Message[] = [createSurface(surfaceId, catalogId)]
  for (let i = 0; i < components.length; i += step) {
    const known = new Set(components.slice(0, i + step).map((c) => c.id))
    // Only reference children that already arrived, so every chunk is a
    // valid surface on its own (a partial answer, never a broken one).
    const chunk = components.slice(0, i + step).map((c) => (Array.isArray(c.children) ? { ...c, children: c.children.filter((id) => known.has(id)) } : c))
    out.push(updateComponents(surfaceId, chunk))
  }
  return out
}

// ---------------------------------------------------------------------------
// Prop controls (the component page's switcher), from the docs props table
// ---------------------------------------------------------------------------

export interface PropDoc {
  name: string
  type: string
  required: boolean
  bindable: boolean
  default?: unknown
  description: string
}

export type Control =
  | { kind: `enum`; name: string; options: string[] }
  | { kind: `boolean`; name: string }
  | { kind: `number`; name: string }
  | { kind: `text`; name: string; multiline: boolean }
  | { kind: `json`; name: string }

const ENUM = /^[A-Za-z0-9_-]+(\s*\|\s*[A-Za-z0-9_-]+)+$/
const TEXTUAL = new Set([`string`, `url`, `icon`, `color`, `date`, `markdown`])

export const enumOptions = (type: string): string[] | null => (ENUM.test(type.trim()) ? type.split(`|`).map((s) => s.trim()) : null)

/** The control a prop gets: enums → segmented/select, booleans → toggle,
 *  numbers → number input, strings → text, everything structured (arrays,
 *  shapes, responsive objects, bindings) → a JSON box when it has a value.
 *  `style` stays out (the JSON shows it). Unknown types: JSON when set. */
export function controlFor(prop: PropDoc, current: unknown): Control | null {
  if (prop.name === `style`) return null
  const isObject = current !== null && typeof current === `object`
  if (isObject) return { kind: `json`, name: prop.name }
  const options = enumOptions(prop.type)
  if (options) return { kind: `enum`, name: prop.name, options }
  if (prop.type === `boolean`) return { kind: `boolean`, name: prop.name }
  if (prop.type === `number`) return { kind: `number`, name: prop.name }
  if (TEXTUAL.has(prop.type)) return { kind: `text`, name: prop.name, multiline: prop.type === `markdown` }
  if (current !== undefined) return { kind: `json`, name: prop.name }
  return null
}

const RESERVED = new Set([`id`, `component`, `children`, `slots`, `on`, `style`, `accessibility`])

/** A flat component's own props (everything but the structural keys). */
export function ownProps(flat: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(flat).filter(([k, v]) => !RESERVED.has(k) && v !== undefined))
}

/** The subject of a specimen's first case: the node named `component`
 *  (the case stack = [caption, subject]). */
export function specimenSubject(node: Nested, component: string): Nested | null {
  const first = node.children?.[0]
  if (!first) return null
  // The case's own children first (a Stack specimen's case IS a Stack), never its caption.
  const hit = (first.children ?? []).find((c) => c.component === component && !c.id.endsWith(`-label`))
  if (hit) return hit
  if (first.component === component) return first
  // A deeper subject (rare): pre-order search.
  const stack: Nested[] = [...(first.children ?? [])]
  while (stack.length) {
    const n = stack.shift()!
    if (n.component === component) return n
    stack.push(...(n.children ?? []), ...Object.values(n.slots ?? {}))
  }
  return null
}

// ---------------------------------------------------------------------------
// The playground's input: messages (JSON array/object or JSONL), a flat
// component list, or a nested node
// ---------------------------------------------------------------------------

export interface ParsedInput {
  messages: Message[]
  /** What the text was read as. */
  form: `messages` | `jsonl` | `flat` | `nested` | `empty`
  errors: string[]
}

const isRecord = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === `object` && !Array.isArray(v)
const isMessage = (v: unknown) => isRecord(v) && MESSAGE_KINDS.some((k) => k in v)
const isComponent = (v: unknown) => isRecord(v) && typeof v.id === `string` && typeof v.component === `string`
const isNested = (v: unknown): boolean =>
  isRecord(v) && isComponent(v) && (`props` in v || (Array.isArray(v.children) && v.children.some(isRecord)) || isRecord(v.slots) && Object.values(v.slots).some(isRecord))

const PLAYGROUND_SURFACE = `main`

export function parseInput(text: string, catalogId = CORE_CATALOG): ParsedInput {
  const trimmed = text.trim()
  if (!trimmed) return { messages: [], form: `empty`, errors: [] }
  let value: unknown
  try {
    value = JSON.parse(trimmed)
  } catch (error) {
    // JSONL: one message per non-empty line.
    const lines = trimmed.split(/\r?\n/).filter((l) => l.trim())
    if (lines.length > 1) {
      const messages: Message[] = []
      const errors: string[] = []
      lines.forEach((line, i) => {
        try {
          const m = JSON.parse(line)
          if (isMessage(m)) messages.push(m as Message)
          else errors.push(`line ${i + 1}: not an A2UI message (expected one of ${MESSAGE_KINDS.join(`, `)})`)
        } catch (e) {
          errors.push(`line ${i + 1}: ${(e as Error).message}`)
        }
      })
      return { messages, form: `jsonl`, errors }
    }
    return { messages: [], form: `empty`, errors: [(error as Error).message] }
  }
  if (Array.isArray(value)) {
    if (value.length > 0 && value.every(isMessage)) return { messages: value as Message[], form: `messages`, errors: [] }
    if (value.length > 0 && value.every(isComponent)) {
      const flat = value as Flat[]
      const errors = flat.some((c) => c.id === `root`) ? [] : [`no component has id "root"`]
      return { messages: surfaceMessages(PLAYGROUND_SURFACE, flat, catalogId), form: `flat`, errors }
    }
    return { messages: [], form: `empty`, errors: [`an array must hold A2UI messages or flat components`] }
  }
  if (isRecord(value) && Array.isArray(value.messages) && value.messages.every(isMessage)) return { messages: value.messages as Message[], form: `messages`, errors: [] }
  if (isMessage(value)) return { messages: [value as Message], form: `messages`, errors: [] }
  if (isNested(value) || isComponent(value)) return { messages: surfaceMessages(PLAYGROUND_SURFACE, flatten(value as Nested), catalogId), form: `nested`, errors: [] }
  return { messages: [], form: `empty`, errors: [`expected A2UI messages, a flat component list or a nested node`] }
}

/** The surface ids the messages address, in first-seen order. */
export function surfaceIds(messages: readonly Message[]): string[] {
  const ids: string[] = []
  for (const m of messages) {
    for (const kind of MESSAGE_KINDS) {
      const body = m[kind] as { surfaceId?: unknown } | undefined
      if (body && typeof body.surfaceId === `string` && !ids.includes(body.surfaceId)) ids.push(body.surfaceId)
    }
  }
  return ids
}

export const pretty = (value: unknown) => JSON.stringify(value, null, 2)
