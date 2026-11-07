// VAPP-200 (spec apps.md §9.2): a READ-ONLY surface. A host whose viewer
// may read but not write (an app at app access `read`) sets
// `SurfaceSettings.readOnly`; every renderer then paints the surface's
// WRITING controls as if their author had written `disabled: true` (the
// components' own disabled look, nothing new). Pure; `fixtures/read-only.json`
// locks it and the Rust core (`eui_core::read_only`) replays it.
//
// A writing control is:
// - any node with an `on.<event>` action that calls a write: `records_*`
//   (the app's rows) or `actions_*` (the package's actions, which write);
//   a Form whose submit writes is disabled, so its fields and buttons are
//   too (the Form's own cascade);
// - a form field with no action of its own: what it edits feeds a later
//   write. A field whose actions are all pure or host calls (a month Select
//   that `bind`s a query, a filter that `set`s) is a VIEW control and stays
//   live.

import type { UiNode } from "./types"

/** The components that edit a value (the Rust core's `FORM_FIELDS`). */
export const READ_ONLY_FIELDS: readonly string[] = [`Input`, `Textarea`, `NumberField`, `Checkbox`, `Radio`, `ChoiceTiles`, `Switch`, `Select`, `ChipInput`, `DatePicker`, `DateRangePicker`, `TimePicker`, `FileUpload`, `Slider`]

/** The call prefixes that write at app access `read`. */
export const READ_ONLY_WRITE_PREFIXES: readonly string[] = [`records_`, `actions_`]

/** Does this function name write (`records_*`, `actions_*`)? */
export function callWrites(name: string): boolean {
  return READ_ONLY_WRITE_PREFIXES.some((p) => name.startsWith(p))
}

/** Does this action (`{functionCall: {"@call"}}`) call a write? */
export function actionWrites(action: unknown): boolean {
  if (!action || typeof action !== `object`) return false
  const call = (action as { functionCall?: { "@call"?: unknown } }).functionCall?.[`@call`]
  return typeof call === `string` && callWrites(call)
}

/** Is `node` a writing control of a read-only surface? */
export function readOnlyDisables(node: Pick<UiNode, `component` | `on`>): boolean {
  const actions = Object.values(node.on ?? {})
  if (actions.length > 0) return actions.some(actionWrites)
  return READ_ONLY_FIELDS.includes(node.component)
}

/** `node` with every writing control disabled (`props.disabled: true`), at
 *  any depth (children and slots). Untouched nodes are shared, not copied;
 *  a renderer runs it over the root AND each lifted template. */
export function readOnlyTree(node: UiNode): UiNode {
  let children = node.children
  for (let i = 0; i < node.children.length; i++) {
    const c = readOnlyTree(node.children[i])
    if (c !== node.children[i]) {
      if (children === node.children) children = node.children.slice()
      children[i] = c
    }
  }
  let slots = node.slots
  if (node.slots)
    for (const [k, s] of Object.entries(node.slots)) {
      const r = readOnlyTree(s)
      if (r !== s) slots = { ...(slots === node.slots ? node.slots : slots), [k]: r }
    }
  const disable = readOnlyDisables(node) && node.props.disabled !== true
  if (!disable && children === node.children && slots === node.slots) return node
  const out: UiNode = { ...node, children }
  if (slots) out.slots = slots
  if (disable) out.props = { ...node.props, disabled: true }
  return out
}

/** The lifted templates of a reduced surface, each read-only. */
export function readOnlyTemplates<T extends Readonly<Record<string, UiNode>>>(templates: T): T {
  let out: Record<string, UiNode> | null = null
  for (const [k, t] of Object.entries(templates)) {
    const r = readOnlyTree(t)
    if (r !== t) (out ??= { ...templates })[k] = r
  }
  return (out ?? templates) as T
}

/** The ids a read-only surface disables (pre-order; the conformance check). */
export function readOnlyDisabledIds(node: UiNode): string[] {
  const out: string[] = []
  const walk = (n: UiNode) => {
    if (readOnlyDisables(n) && n.props.disabled !== true) out.push(n.id)
    for (const s of Object.values(n.slots ?? {})) walk(s)
    n.children.forEach(walk)
  }
  walk(node)
  return out
}
