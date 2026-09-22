// EXP-998: the BLOCKS RAIL — the git-graph-style column at the right edge of
// an issue list that draws every open `blocks` relation between two VISIBLE
// rows as an arrow from the blocker to the issue it blocks. It replaces the
// per-row counts badge at md+ (a phone keeps the badge: its rows are too
// narrow for a rail, and the natives draw the badge too).
//
// The rule is pure over the list's VISIBLE ENTRIES in order — issue rows,
// group headers and "Show more" buttons alike — because, like the tree
// connector (EXP-965), every entry can only paint inside itself: an edge that
// crosses a group header is drawn as one slice per entry it passes. A row
// hidden by a fold or behind a cap is simply absent, so an edge to it is not
// drawn; the row's node still tells (its ring) that something is in its way,
// and the mini-graph behind it shows what.
//
// Lanes are assigned git-graph style: every edge is an interval over entry
// indices, walked in order, taking the lowest lane free at its start. Two
// edges may SHARE a lane when they only touch at one row (a chain A → B → C
// reads as one line with an arrowhead at each blocked node); overlapping
// edges never do. The mirrored ×4 rule lives here; the desktop paints it in
// `crates/ui/src/issue_rail.rs` over `domain::issue_rail`.

import type { BlockCounts, GraphIssue, GraphRelation } from "./issue-graph"
import { openBlockEdges } from "./issue-graph"

/** One visible entry of the list, top to bottom. */
export type RailEntry =
  | { kind: `row`; id: string }
  /** A group header, a "Show more" button: lanes pass, nothing else. */
  | { kind: `gap` }

/** What a lane draws at ONE entry. */
export interface RailLane {
  lane: number
  /** A vertical from the entry's top edge to its centre. */
  top: boolean
  /** A vertical from the entry's centre to its bottom edge. */
  bottom: boolean
  /** The lane joins this row's node: an arrowhead INTO the node (this row is
   *  blocked by the edge's other end). */
  into: boolean
  /** The lane leaves this row's node (this row blocks the other end). */
  out: boolean
  /** Part of a blocking cycle — drawn red. */
  cycle: boolean
  /** The edges (`from:to` id keys + `EXP-1 blocks EXP-2` labels) using this
   *  lane at this entry — the hover target. */
  edges: RailLaneEdge[]
}

export interface RailLaneEdge {
  key: string
  label: string
}

export type RailNode = `blocked` | `blocking`

export interface RailRow {
  /** The node dot: `blocked` = an open blocker exists (a ring), `blocking` =
   *  only in something else's way (a dot); `null` = no open relation. */
  node: RailNode | null
  /** Open blockers / open blocked issues, visible or not — the label. */
  counts: BlockCounts
  lanes: RailLane[]
}

export interface IssueRail {
  /** One per entry, in order. A gap entry carries only pass-through lanes. */
  entries: RailRow[]
  /** How many lanes the rail needs; 0 = no arrow to draw. */
  laneCount: number
  /** Any row with a node at all — the rail column shows when true. */
  hasNodes: boolean
}

/** The node column's width: an 8px gutter off the cell before it (the due
 *  date), then the dot centred in the remaining 16. */
export const RAIL_NODE_WIDTH = 24
/** One lane's width. */
export const RAIL_LANE_PITCH = 8
/** The arrow's corner radius — the tree connector's (`TREE_RADIUS`). */
export const RAIL_RADIUS = 3

/** The rail column's width for a lane count. */
export function railWidth(rail: Pick<IssueRail, `laneCount` | `hasNodes`>): number {
  if (!rail.hasNodes) return 0
  return RAIL_NODE_WIDTH + RAIL_LANE_PITCH * rail.laneCount
}

/** The x of the node dot's centre inside the rail. */
export const RAIL_NODE_X = RAIL_NODE_WIDTH - 8
/** The x of lane `lane`'s centre inside the rail. */
export function railLaneX(lane: number): number {
  return RAIL_NODE_WIDTH + RAIL_LANE_PITCH * lane + RAIL_LANE_PITCH / 2
}

export const edgeKey = (from: string, to: string) => `${from}:${to}`

/** `EXP-1 blocks EXP-2` — the edge's hover label. Byte-identical ×4. */
export function railEdgeLabel(from: string, to: string): string {
  return `${from} blocks ${to}`
}

/** Strongly connected components over the open edges (Tarjan): an edge whose
 *  ends share a component sits on a blocking cycle. */
function cycleEdges(edges: readonly { from: string; to: string }[]): Set<string> {
  const out = new Map<string, string[]>()
  for (const edge of edges) {
    out.set(edge.from, [...(out.get(edge.from) ?? []), edge.to])
    if (!out.has(edge.to)) out.set(edge.to, [])
  }
  const index = new Map<string, number>()
  const low = new Map<string, number>()
  const component = new Map<string, number>()
  const stack: string[] = []
  const onStack = new Set<string>()
  let next = 0
  let components = 0
  const visit = (start: string) => {
    // Iterative Tarjan: a deep chain must not overflow the call stack.
    const frames: Array<{ id: string; at: number }> = [{ id: start, at: 0 }]
    index.set(start, next)
    low.set(start, next)
    next += 1
    stack.push(start)
    onStack.add(start)
    while (frames.length > 0) {
      const frame = frames[frames.length - 1]!
      const targets = out.get(frame.id) ?? []
      if (frame.at < targets.length) {
        const target = targets[frame.at]!
        frame.at += 1
        if (!index.has(target)) {
          index.set(target, next)
          low.set(target, next)
          next += 1
          stack.push(target)
          onStack.add(target)
          frames.push({ id: target, at: 0 })
        } else if (onStack.has(target)) {
          low.set(frame.id, Math.min(low.get(frame.id)!, index.get(target)!))
        }
        continue
      }
      frames.pop()
      const parent = frames[frames.length - 1]
      if (parent) {
        low.set(parent.id, Math.min(low.get(parent.id)!, low.get(frame.id)!))
      }
      if (low.get(frame.id) === index.get(frame.id)) {
        let member: string
        do {
          member = stack.pop()!
          onStack.delete(member)
          component.set(member, components)
        } while (member !== frame.id)
        components += 1
      }
    }
  }
  for (const id of out.keys()) {
    if (!index.has(id)) visit(id)
  }
  const cyclic = new Set<string>()
  for (const edge of edges) {
    if (component.get(edge.from) === component.get(edge.to)) {
      cyclic.add(edgeKey(edge.from, edge.to))
    }
  }
  return cyclic
}

/**
 * The rail for `entries`, given the team's relations and issues (the same
 * open-edge rule as the counts badge, `lib/issue-graph.ts`).
 */
export function issueRail(
  entries: readonly RailEntry[],
  relations: readonly GraphRelation[],
  issues: readonly GraphIssue[]
): IssueRail {
  const at = new Map<string, number>()
  entries.forEach((entry, index) => {
    if (entry.kind === `row` && !at.has(entry.id)) at.set(entry.id, index)
  })

  const open = openBlockEdges(relations, issues)
  const cyclic = cycleEdges(open)
  const identifierOf = new Map(issues.map((row) => [row.id, row.identifier]))
  const counts = new Map<string, BlockCounts>()
  const countAt = (id: string) => {
    let entry = counts.get(id)
    if (!entry) {
      entry = { blockedBy: 0, blocking: 0 }
      counts.set(id, entry)
    }
    return entry
  }
  for (const edge of open) {
    countAt(edge.from).blocking += 1
    countAt(edge.to).blockedBy += 1
  }

  // The drawable edges: both ends visible. Ordered by their upper end, then
  // by their lower end, then by key — so the lane walk is deterministic.
  const spans = open
    .filter((edge) => at.has(edge.from) && at.has(edge.to))
    .map((edge) => {
      const from = at.get(edge.from)!
      const to = at.get(edge.to)!
      return {
        key: edgeKey(edge.from, edge.to),
        label: railEdgeLabel(
          identifierOf.get(edge.from) ?? edge.from,
          identifierOf.get(edge.to) ?? edge.to
        ),
        from,
        to,
        lo: Math.min(from, to),
        hi: Math.max(from, to),
        cycle: cyclic.has(edgeKey(edge.from, edge.to)),
      }
    })
    .sort((a, b) => a.lo - b.lo || a.hi - b.hi || (a.key < b.key ? -1 : 1))

  // Greedy lanes: the lowest lane whose last edge ENDED at or above this
  // edge's start (touching at one row is sharing, overlapping is not).
  const laneEnd: number[] = []
  const lanes = spans.map((span) => {
    let lane = laneEnd.findIndex((end) => end <= span.lo)
    if (lane === -1) {
      lane = laneEnd.length
      laneEnd.push(span.hi)
    } else {
      laneEnd[lane] = span.hi
    }
    return lane
  })

  const rows: RailRow[] = entries.map(() => ({
    node: null,
    counts: { blockedBy: 0, blocking: 0 },
    lanes: [],
  }))
  const laneAt = (index: number, lane: number) => {
    const row = rows[index]!
    let slot = row.lanes.find((candidate) => candidate.lane === lane)
    if (!slot) {
      slot = {
        lane,
        top: false,
        bottom: false,
        into: false,
        out: false,
        cycle: false,
        edges: [],
      }
      row.lanes.push(slot)
      row.lanes.sort((a, b) => a.lane - b.lane)
    }
    return slot
  }
  spans.forEach((span, index) => {
    const lane = lanes[index]!
    for (let entry = span.lo; entry <= span.hi; entry += 1) {
      const slot = laneAt(entry, lane)
      slot.edges.push({ key: span.key, label: span.label })
      slot.cycle ||= span.cycle
      if (entry > span.lo) slot.top = true
      if (entry < span.hi) slot.bottom = true
      if (entry === span.from) slot.out = true
      if (entry === span.to) slot.into = true
    }
  })

  let hasNodes = false
  entries.forEach((entry, index) => {
    if (entry.kind !== `row`) return
    const count = counts.get(entry.id)
    if (!count) return
    const row = rows[index]!
    row.counts = count
    row.node = count.blockedBy > 0 ? `blocked` : `blocking`
    hasNodes = true
  })

  return { entries: rows, laneCount: laneEnd.length, hasNodes }
}

/** Nothing to draw for this entry. */
export function railRowIsEmpty(row: RailRow | null | undefined): boolean {
  return !row || (row.node === null && row.lanes.length === 0)
}
