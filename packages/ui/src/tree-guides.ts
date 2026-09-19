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
//     vertical from the row's top edge down to its vertical centre, a 5px
//     rounded corner turning right, and a 1px stub out to the gutter's right
//     edge (just before the child's own glyph);
//   * that vertical continues to the row's BOTTOM edge (a tee) when the row is
//     not the LAST child of its parent among the visible rows;
//   * for every ancestor level whose subtree continues after this row, a
//     straight full-height vertical at that level's gutter centre;
//   * a parent row draws nothing of its own; a folded subtree draws nothing.

/** Guide geometry for ONE row, in gutter LEVELS (not pixels). */
export interface TreeGuide {
  /** The gutter level the elbow sits in (`depth - 1`); `null` for a root. */
  elbowAt: number | null
  /** The elbow's vertical continues past the centre: a later sibling follows. */
  tee: boolean
  /** Ancestor gutter levels carrying a straight full-height line, ascending. */
  passThrough: number[]
}

/** 14px per nesting level — the ×4 indent. */
export const TREE_INDENT = 14
/** The first gutter starts here: `ListRow`'s own 12px left padding. */
export const TREE_BASE = 12
/** The elbow's corner radius. */
export const TREE_RADIUS = 5

/** The x centre of gutter level `level`. `base` is the list's own left
 * padding — `TREE_BASE` for every list row, `0` for the few nestings that
 * indent from a container's edge (the PR stack inside the graph overlay). */
export function treeGuideCentre(level: number, base = TREE_BASE): number {
  return base + TREE_INDENT * level + TREE_INDENT / 2
}

/**
 * Whether the subtree hanging off gutter `level` continues AFTER row `index`
 * — i.e. another row at depth `level + 1` follows before the walk leaves that
 * subtree (a row at depth `≤ level`).
 */
function continuesAfter(
  depths: readonly number[],
  index: number,
  level: number
): boolean {
  for (let at = index + 1; at < depths.length; at += 1) {
    const depth = depths[at]!
    if (depth <= level) return false
    if (depth === level + 1) return true
  }
  return false
}

/**
 * The guides for a flat list of VISIBLE rows, given their depths (a folded
 * subtree is simply absent from the array, which is what makes a collapsed
 * parent draw nothing below it).
 */
export function treeGuides(depths: readonly number[]): TreeGuide[] {
  return depths.map((depth, index) => {
    if (depth <= 0) return { elbowAt: null, tee: false, passThrough: [] }
    const passThrough: number[] = []
    for (let level = 0; level <= depth - 2; level += 1) {
      if (continuesAfter(depths, index, level)) passThrough.push(level)
    }
    return {
      elbowAt: depth - 1,
      tee: continuesAfter(depths, index, depth - 1),
      passThrough,
    }
  })
}

/** Nothing to draw — a root row with no ancestors still carrying a line. */
export function treeGuideIsEmpty(guide: TreeGuide | null | undefined): boolean {
  return (
    !guide || (guide.elbowAt === null && guide.passThrough.length === 0)
  )
}
