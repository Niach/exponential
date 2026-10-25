/* The nested authoring form of an A2UI surface and its flattening into
   A2UI v0.9 messages (the same two helpers exponential.at/ui uses,
   ui/src/sdk/a2ui.ts). Pure: a post's concept drafts are written
   as `Nested` trees and the live island feeds the messages to the renderer. */

export const A2UI_VERSION = `v0.9`
export const CORE_CATALOG = `https://ui.exponential.at/catalogs/core/v1`

export interface Nested {
  id: string
  component: string
  props?: Record<string, unknown>
  style?: Record<string, unknown>
  children?: Nested[]
}

export interface Flat {
  id: string
  component: string
  children?: string[]
  [prop: string]: unknown
}

export interface Message {
  version: string
  createSurface?: { surfaceId: string; catalogId: string }
  updateComponents?: { surfaceId: string; components: Flat[] }
}

/** The nested form → flat components, pre-order; the top node is `root`. */
export function flatten(tree: Nested, rootId = `root`): Flat[] {
  const out: Flat[] = []
  const walk = (n: Nested, id: string) => {
    const flat: Flat = { id, component: n.component, ...(n.props ?? {}) }
    if (n.children?.length) flat.children = n.children.map((c) => c.id)
    if (n.style) flat.style = n.style
    out.push(flat)
    for (const c of n.children ?? []) walk(c, c.id)
  }
  walk(tree, rootId)
  return out
}

/** A complete surface as messages: createSurface + one updateComponents. */
export function surfaceMessages(surfaceId: string, tree: Nested): Message[] {
  return [
    { version: A2UI_VERSION, createSurface: { surfaceId, catalogId: CORE_CATALOG } },
    { version: A2UI_VERSION, updateComponents: { surfaceId, components: flatten(tree) } },
  ]
}
