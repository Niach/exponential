/* The playground's A2UI helpers (pure, no renderer, no React): the nested
   authoring form → the flat A2UI v0.9 `updateComponents` list, the message
   envelope, and the editor's input parser (messages as a JSON array/object
   or JSONL, a flat component list, or a nested node). */

export const A2UI_VERSION = `v0.9`
export const CORE_CATALOG = `https://ui.exponential.at/catalogs/core/v1`

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

/** One component as it rides `updateComponents`. */
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

/** The nested form → flat components, pre-order; the top node becomes `rootId`. */
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
export const surfaceMessages = (surfaceId: string, components: Flat[], catalogId = CORE_CATALOG): Message[] => [createSurface(surfaceId, catalogId), updateComponents(surfaceId, components)]

/** A streamed replay: createSurface, then updateComponents in growing
 *  chunks; each chunk references only children that already arrived. */
export function replayChunks(surfaceId: string, components: Flat[], step = 2, catalogId = CORE_CATALOG): Message[] {
  const out: Message[] = [createSurface(surfaceId, catalogId)]
  for (let i = 0; i < components.length; i += step) {
    const known = new Set(components.slice(0, i + step).map((c) => c.id))
    const chunk = components.slice(0, i + step).map((c) => (Array.isArray(c.children) ? { ...c, children: c.children.filter((id) => known.has(id)) } : c))
    out.push(updateComponents(surfaceId, chunk))
  }
  return out
}

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
  isRecord(v) && isComponent(v) && (`props` in v || (Array.isArray(v.children) && v.children.some(isRecord)) || (isRecord(v.slots) && Object.values(v.slots).some(isRecord)))

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

/** The messages that address one surface. */
export const messagesFor = (messages: readonly Message[], surfaceId: string): Message[] =>
  messages.filter((m) => MESSAGE_KINDS.some((k) => (m[k] as { surfaceId?: unknown } | undefined)?.surfaceId === surfaceId))
