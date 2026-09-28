// VAPP-4 spike (throwaway): the ONE kitchen-sink tree every client renders.
// The web lane paints it with real CSS; the natives go through the taffy core
// (`apps/desktop/crates/vapp-spike`).
import fixtureJson from "@exp/domain-contract/fixtures/vapp-kitchen-sink.json"

export type VappKind =
  | `box`
  | `card`
  | `text`
  | `button`
  | `textfield`
  | `textarea`
  | `toggle`
  | `select`
  | `listrow`
  | `badge`
  | `pill`
  | `avatar`
  | `image`
  | `divider`
  | `progress`
  | `markdown`

/** A style value: number (px), string (%, keyword, track list, colour), a
 *  string[] (`gridTemplateAreas` rows) or a nested condition object. */
export type VappStyleValue = number | string | Array<string> | VappStyle
export type VappStyle = { [key: string]: VappStyleValue }

export interface VappNode {
  id: string
  kind: VappKind
  style?: VappStyle
  props?: Record<string, unknown>
  children?: Array<VappNode>
}

export const CONTAINER_KINDS: ReadonlySet<VappKind> = new Set([`box`, `card`])

export const kitchenSink = fixtureJson as unknown as VappNode

/** Pre-order walk (= paint order = a11y order, same as the core). */
export function walk(node: VappNode, visit: (n: VappNode) => void) {
  visit(node)
  for (const c of node.children ?? []) walk(c, visit)
}
