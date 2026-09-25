// EXP-998 → EXP-1057: the BLOCKS RAIL — the column at the right edge of an
// issue list that marks every row in an open `blocks` relation with a DOT.
// It replaces the per-row counts badge at md+ (a phone keeps the badge: its
// rows are too narrow for a rail, and the natives draw the badge too).
//
// EXP-1057 dropped the git-graph lanes and arrows between rows: they cost a
// column per overlapping edge and their ends sat too far apart to read.
// Hovering a dot opens THE mini-graph (`components/issue-graph.tsx`) instead,
// which shows the whole picture. The desktop mirrors this in
// `domain::issue_rail`; the geometry is `issue-graph-geometry.json`'s.

import { ISSUE_GRAPH_GEOMETRY, type BlockCounts } from "./issue-graph"

export type RailNode = `blocked` | `blocking`

/** The node column, at the rail's RIGHT edge: the dot centred in it. */
export const RAIL_NODE_WIDTH = ISSUE_GRAPH_GEOMETRY.railNodeWidth
/** The blank between the cell before the rail (the due date) and the dot. */
export const RAIL_GUTTER = ISSUE_GRAPH_GEOMETRY.railGutter

/** A row's dot: `blocked` = an open blocker exists (a ring), `blocking` =
 *  only in something else's way (a filled dot); `null` = no open relation. */
export function railNode(counts: BlockCounts | undefined): RailNode | null {
  if (!counts) return null
  if (counts.blockedBy > 0) return `blocked`
  if (counts.blocking > 0) return `blocking`
  return null
}

/** The rail column's width for a list: 0 when none of its `ids` has a dot. */
export function railWidth(
  ids: Iterable<string>,
  counts: ReadonlyMap<string, BlockCounts>
): number {
  for (const id of ids) {
    if (railNode(counts.get(id)) !== null) return RAIL_GUTTER + RAIL_NODE_WIDTH
  }
  return 0
}
