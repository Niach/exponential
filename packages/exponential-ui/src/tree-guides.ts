// Round 3 (VAPP-102, docs/round-3-contract.md §2): the tree connector rule
// the CORE computes for nested rows. `treeGuides(depths)` is the ×4 rule of
// the app (`packages/ui/src/tree-guides.ts`, desktop `domain::tree_guides`,
// iOS `TreeGuides.swift`, Android `TreeGuides.kt`), copied verbatim so a
// Section of Rows connects like a session tree or a PR stack. `applyTreeGuides`
// is the ONE post-expansion pass (the end of `expandMacros`; the Rust core
// mirrors it in `tree_guides.rs`): over every node's children in order, each
// Row root contributes its numeric `recipe.props.depth` (anything else counts
// as 0 and ends every subtree), and the rule fills the Row's `guides` part (a
// TreeGuides native) with `elbowAt` / `tee` / `passThrough`. Locked by
// `fixtures/tree-guides.json` and the Row cases of `catalog-macros.json`.

import type { UiNode } from "./types"

/** Guide geometry for ONE row, in gutter LEVELS (not pixels). */
export interface TreeGuide {
  /** The gutter level the elbow sits in (`depth - 1`); `null` for a root. */
  elbowAt: number | null
  /** The elbow's vertical continues past the centre: a later sibling follows. */
  tee: boolean
  /** Ancestor gutter levels carrying a straight full-height line, ascending. */
  passThrough: number[]
}

/** Whether the subtree hanging off gutter `level` continues AFTER row `index`:
 *  another row at depth `level + 1` follows before the walk leaves that
 *  subtree (a row at depth `≤ level`). */
function continuesAfter(depths: readonly number[], index: number, level: number): boolean {
  for (let at = index + 1; at < depths.length; at += 1) {
    const depth = depths[at]!
    if (depth <= level) return false
    if (depth === level + 1) return true
  }
  return false
}

/** The guides for a flat list of VISIBLE rows, given their depths. */
export function treeGuides(depths: readonly number[]): TreeGuide[] {
  return depths.map((depth, index) => {
    if (depth <= 0) return { elbowAt: null, tee: false, passThrough: [] }
    const passThrough: number[] = []
    for (let level = 0; level <= depth - 2; level += 1) {
      if (continuesAfter(depths, index, level)) passThrough.push(level)
    }
    return { elbowAt: depth - 1, tee: continuesAfter(depths, index, depth - 1), passThrough }
  })
}

/** The expanded root of a `Row` macro. */
export function isRowRoot(node: UiNode): boolean {
  return node.recipe?.macro === `Row` && node.recipe.part === `root`
}

/** A Row root's depth: its recipe prop when a finite number ≥ 0, else 0 (a
 *  bound depth cannot be computed against its siblings). */
function rowDepth(node: UiNode): number {
  const depth = node.recipe?.props.depth
  return typeof depth === `number` && Number.isFinite(depth) && depth > 0 ? Math.floor(depth) : 0
}

/** The Row's `guides` part, when its depth emitted one. */
function guidesPart(row: UiNode): UiNode | undefined {
  return row.children.find((c) => c.component === `TreeGuides` && c.recipe?.part === `guides`)
}

/** Fill every Row's guides from the rows around it. Mutates `root` in place
 *  (the expander hands it a fresh tree); returns it for chaining. */
export function applyTreeGuides(root: UiNode): UiNode {
  const visit = (node: UiNode) => {
    const depths = node.children.map((c) => (isRowRoot(c) ? rowDepth(c) : 0))
    const guides = treeGuides(depths)
    node.children.forEach((child, i) => {
      if (!isRowRoot(child)) return
      const part = guidesPart(child)
      if (!part) return
      const g = guides[i]!
      const props: Record<string, unknown> = { depth: depths[i] }
      if (g.elbowAt !== null) props.elbowAt = g.elbowAt
      props.tee = g.tee
      props.passThrough = g.passThrough
      part.props = props
    })
    for (const slot of Object.values(node.slots ?? {})) visit(slot)
    node.children.forEach(visit)
  }
  visit(root)
  return root
}
