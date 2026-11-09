// #EDE-65 lane M: Embed, a nested surface drawn in place (fixtures/embed.json;
// the Rust core `eui_core::embed`, SwiftUI `EmbedCard`, Compose `EmbedCard`
// replay the same file). The pure half every painter shares: which `surface`
// value is a card, the error box a bad one shows instead, and what an inner
// event becomes when it reaches the host as the Embed's `action`.

import type { NestedNode } from "./types"
import { A2UI_VERSION } from "./catalog"

/** A nested card: the root and its OWN data model (never the host's). */
export interface EmbedCard {
  root: NestedNode
  data: Record<string, unknown>
}

/** What a bad `surface` paints instead (`fixtures/embed.json` `errorRoot`):
 *  an error Alert; the host around it keeps painting. */
export const EMBED_ERROR_ROOT: NestedNode = { id: `embed_error`, component: `Alert`, props: { type: `error`, title: `$string.embedInvalid` } }

/** The Embed's one event: every inner `event` action reaches the host as it. */
export const EMBED_EVENT = `action`

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** A structurally sound nested node: string `id` + `component`, `props`
 *  / `on` / `slots` objects, `children` an array of nodes, recursively. */
function soundNode(n: unknown, depth: number): boolean {
  if (!isObj(n) || depth > 64) return false
  if (typeof n.id !== `string` || n.id === `` || typeof n.component !== `string` || n.component === ``) return false
  for (const key of [`props`, `on`, `style`] as const) if (n[key] !== undefined && !isObj(n[key])) return false
  if (n.children !== undefined && (!Array.isArray(n.children) || !n.children.every((c) => soundNode(c, depth + 1)))) return false
  if (n.slots !== undefined && (!isObj(n.slots) || !Object.values(n.slots).every((c) => soundNode(c, depth + 1)))) return false
  return true
}

/** The Embed's resolved `surface` prop → its card, or `undefined` when it is
 *  not one (not an object, no sound `root`, a non-object `data`, a `version`
 *  other than 1.0): the painter then shows {@link EMBED_ERROR_ROOT}. A
 *  missing or null `data` is an empty model. An unknown component inside a
 *  sound root is NOT bad: the nested surface paints its Unknown placeholder. */
export function embedCard(surface: unknown): EmbedCard | undefined {
  if (!isObj(surface)) return undefined
  if (surface.version !== undefined && surface.version !== A2UI_VERSION) return undefined
  if (!soundNode(surface.root, 1)) return undefined
  const data = surface.data
  if (data !== undefined && data !== null && !isObj(data)) return undefined
  return { root: surface.root as unknown as NestedNode, data: isObj(data) ? data : {} }
}

/** The card a painter mounts for `surface`: the card, else the error box. */
export function embedMount(surface: unknown): { card: EmbedCard; valid: boolean } {
  const card = embedCard(surface)
  return card ? { card, valid: true } : { card: { root: EMBED_ERROR_ROOT, data: {} }, valid: false }
}

/** An inner event → the Embed's `action` payload (merged over the author's
 *  context like any component payload): the inner name and context. */
export function embedPayload(event: { name: string; context?: Record<string, unknown> }): { name: string; context: Record<string, unknown> } {
  return { name: event.name, context: { ...(event.context ?? {}) } }
}

/** The nested surface's id: the host's surface id and the Embed's (painted)
 *  component id, so two cards' inner ids never meet. */
export function embedSurfaceId(hostSurfaceId: string, componentId: string): string {
  return `${hostSurfaceId}/${componentId}`
}
