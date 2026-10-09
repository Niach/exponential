// EXP-965: the TREE CONNECTOR rule — every nested list on every client draws
// Linear-style guide lines instead of bare indentation. ONE pure function,
// mirrored ×4 (desktop `domain::tree_guides`, iOS `TreeGuides.swift`, Android
// `TreeGuides.kt`) so a session tree, a PR stack and a run tree all connect
// the same way.
//
// The geometry (shared):
//
//   * indent stays 14px per level; a row at depth `d` is padded to
//     `12 + 14 * d`;
//   * gutter level `L` is the 14px band from `12 + 14 * L` to `12 + 14*(L+1)`,
//     its centre (`12 + 14*L + 7`) aligned with the horizontal centre of the
//     leading glyph box of a row AT that depth — so a child's elbow hangs off
//     its parent's glyph;
//   * a row at depth `d ≥ 1` draws the ELBOW in gutter level `d - 1`: a 1px
//     vertical from the row's top edge down to its vertical centre, a 3px
//     rounded corner turning right, and a 1px stub out to the gutter's right
//     edge (just before the child's own glyph);
//   * that vertical continues to the row's BOTTOM edge (a tee) when the row is
//     not the LAST child of its parent among the visible rows — ONE unbroken
//     line from top to bottom with the corner branching off it (EXP-998: a
//     vertical that stopped at the corner and resumed at the centre left a
//     notch where the arc bowed out);
//   * for every ancestor level whose subtree continues after this row, a
//     straight full-height vertical at that level's gutter centre;
//   * a parent row draws nothing of its own; a folded subtree draws nothing.

// VAPP-102: the RULE lives once, in the SDK (`@exponential-at/ui`
// `treeGuides`, the core's post-expansion pass fills a catalog Row's guides
// with it; Rust `tree_guides.rs`, fixture `tree-guides.json`). The app
// re-exports it rather than keeping a second copy. The geometry numbers below
// stay the app's own literals (its pixels are shots-locked) and are asserted
// equal to the catalog's layout numbers in tree-guides.test.ts.
export { treeGuides, type TreeGuide } from "@exponential-at/ui"
import type { TreeGuide } from "@exponential-at/ui"

/** 14px per nesting level — the ×4 indent (= the catalog's
 *  `TREE_GUIDE_COLUMN`). */
export const TREE_INDENT = 14
/** The first gutter starts here: `ListRow`'s own 12px left padding. */
export const TREE_BASE = 12
/** The elbow's corner radius — a tight, "edgy" turn (EXP-998: 5 read as a
 *  bulge on a 28px row), the ×4 number (= the catalog's `TREE_GUIDE_RADIUS`). */
export const TREE_RADIUS = 3

/** The x centre of gutter level `level`. `base` is the list's own left
 * padding — `TREE_BASE` for every list row, `0` for the few nestings that
 * indent from a container's edge (the PR stack inside the graph overlay). */
export function treeGuideCentre(level: number, base = TREE_BASE): number {
  return base + TREE_INDENT * level + TREE_INDENT / 2
}

/** Nothing to draw — a root row with no ancestors still carrying a line. */
export function treeGuideIsEmpty(guide: TreeGuide | null | undefined): boolean {
  return (
    !guide || (guide.elbowAt === null && guide.passThrough.length === 0)
  )
}
