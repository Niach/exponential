import { useMemo, type ReactNode } from "react"
import { cn } from "./cn"

// EXP-1033 — the wave GRID, moved here from the app's `components/
// issue-graph.tsx` so the two graphs that draw it (the blocks mini-graph and
// the workflow screen) share ONE drawing half, and the styleguide can render
// it as a real island.
//
// No graph library: every node arrives with a `wave` (column) and a `lane`
// (row) — derived by the app's `lib/issue-graph.ts`, synced from the server
// for a workflow (EXP-981) — so this is a grid of boxes with one SVG of edges
// behind it. An edge runs from its blocker's right-middle port to the blocked
// node's left-middle port, bowed inside the GAP between the two columns so it
// never cuts through a box.
//
// EXP-983: every edge arrives with its STYLE rather than a flag per meaning —
// grey solid, red on a cycle or a stale upstream, green out of a landed node,
// grey DASHED while the dependent builds on work nobody landed yet. The app's
// `lib/workflow-view.ts` decides which (`workflowEdgeStyle`) and re-exports
// the type from here, so the drawing contract has ONE home.

/** How an edge is drawn. Grey solid is the default; the others say something. */
export type WorkflowEdgeStyle =
  | `plain`
  | `cycle`
  | `stale`
  | `landed`
  | `speculative`

const NODE_W = 172
const NODE_H = 28
const WAVE_GAP = 40
const LANE_GAP = 8

/** The one thing the grid needs of a node: where it sits. */
export interface WaveGraphNode {
  id: string
  wave: number
  lane: number
}

export interface WaveGraphEdge {
  /** The blocker. */
  from: string
  to: string
  /** EXP-983: how the edge is drawn, decided by the caller — the workflow
   *  graph through `workflowEdgeStyle`, the issue flavour with nothing but
   *  `plain` and `cycle` (a blocking cycle) to say. */
  style: WorkflowEdgeStyle
}

/** The paint per style. `speculative` is the only dashed one: the dependent
 *  started on work its blocker has not landed yet. */
const EDGE_STROKE: Record<WorkflowEdgeStyle, string> = {
  plain: `var(--glass-stroke-strong)`,
  cycle: `var(--destructive)`,
  stale: `var(--destructive)`,
  landed: `var(--color-emerald-500)`,
  speculative: `var(--glass-stroke-strong)`,
}

/** Per-node chrome the caller owns: the positioned box's classes and its
 *  test id (which names the ISSUE on the issue flavour, the NODE otherwise). */
export interface WaveGraphNodeProps {
  className?: string
  testId?: string
  /** The cell paints outside its box (a ping halo, a pick ring, a stack). */
  overflowVisible?: boolean
}

/** The grid's metrics — every one of them optional, and every default the
 *  issue flavour's. */
export interface WaveGraphMetrics {
  nodeWidth?: number
  nodeHeight?: number
  waveGap?: number
  laneGap?: number
}

/** What a custom edge painter (`pathFor`) sees: both nodes, where their
 *  boxes sit in the grid's own coordinates (no inset; the host pads), and
 *  the metrics the grid was laid out with. */
export interface WaveGraphEdgeGeometry extends Required<WaveGraphMetrics> {
  from: WaveGraphNode
  to: WaveGraphNode
  /** The blocker box's top-left. */
  fromAt: { x: number; y: number }
  /** The blocked box's top-left. */
  toAt: { x: number; y: number }
}

/** The default edge: a level stub from the anchor to its cell's edge, then a
 *  cubic bowed inside the GAP between the two columns, so an edge never cuts
 *  through a label that sits beside (or under) its anchor. A cycle edge that
 *  runs backwards (or inside one wave) has no gap to curve in and bows half a
 *  gap instead. */
function defaultEdgePath(
  geometry: WaveGraphEdgeGeometry,
  out: { x: number; y: number },
  into: { x: number; y: number }
): string {
  const { fromAt, toAt, nodeWidth, waveGap } = geometry
  const x1 = fromAt.x + out.x
  const y1 = fromAt.y + out.y
  const x2 = toAt.x + into.x
  const y2 = toAt.y + into.y
  const gapStart = fromAt.x + nodeWidth
  const gapEnd = toAt.x
  const forward = gapEnd > gapStart
  const c1 = forward ? gapStart : x1
  const c2 = forward ? gapEnd : x2
  const bend = forward
    ? (c2 - c1) / 2
    : Math.max(waveGap / 2, Math.abs(x2 - x1) / 2)
  return `M ${x1} ${y1} L ${c1} ${y1} C ${c1 + bend} ${y1}, ${c2 - bend} ${y2}, ${c2} ${y2} L ${x2} ${y2}`
}

/** The default stroke; the issue flavour passes the contract's constant. */
const EDGE_STROKE_WIDTH = 1.25

/**
 * The grid's NATURAL size, with nothing drawn: a host that has to fit the
 * whole picture into a fixed width (the workflow screen scales it down rather
 * than growing an inner scrollbar) measures it before it renders.
 */
export function waveGraphSize(
  nodes: readonly WaveGraphNode[],
  metrics: WaveGraphMetrics = {}
): { width: number; height: number } {
  const nodeWidth = metrics.nodeWidth ?? NODE_W
  const nodeHeight = metrics.nodeHeight ?? NODE_H
  const waveGap = metrics.waveGap ?? WAVE_GAP
  const laneGap = metrics.laneGap ?? LANE_GAP
  let waves = 0
  let lanes = 0
  for (const node of nodes) {
    waves = Math.max(waves, node.wave + 1)
    lanes = Math.max(lanes, node.lane + 1)
  }
  return {
    width: Math.max(0, waves * (nodeWidth + waveGap) - waveGap),
    height: Math.max(0, lanes * (nodeHeight + laneGap) - laneGap),
  }
}

/**
 * The bare wave GRID: columns are waves, rows are lanes, one SVG of bowed
 * edges behind the boxes. It draws and never lays out — the layout arrives
 * with the nodes. `renderNode` returning null drops that box entirely (an
 * issue row that has not synced yet).
 */
export function WaveGraph({
  nodes,
  edges,
  nodeWidth = NODE_W,
  nodeHeight = NODE_H,
  waveGap = WAVE_GAP,
  laneGap = LANE_GAP,
  edgeOut,
  edgeIn,
  edgeStrokeWidth = EDGE_STROKE_WIDTH,
  pathFor,
  idPrefix = `issue-graph`,
  renderNode,
  nodeProps,
}: {
  nodes: readonly WaveGraphNode[]
  edges: readonly WaveGraphEdge[]
  nodeWidth?: number
  nodeHeight?: number
  waveGap?: number
  laneGap?: number
  /** Where an edge LEAVES and ENTERS a cell, as offsets from its top-left.
   *  Boxes default to their side middles. */
  edgeOut?: { x: number; y: number }
  edgeIn?: { x: number; y: number }
  /** The edges' stroke width. */
  edgeStrokeWidth?: number
  /** EXP-1057: the edge's SVG path, when the caller owns the curve (the
   *  issue flavour draws THE contract's cubic, `lib/issue-graph.ts`
   *  `issueGraphEdge`, so the web and the natives bend alike). Absent, the
   *  grid bows the edge inside the gap itself. */
  pathFor?: (edge: WaveGraphEdge, geometry: WaveGraphEdgeGeometry) => string
  /** Names the edge paths and any unnamed node box in the DOM. */
  idPrefix?: string
  renderNode: (id: string) => ReactNode
  /** `onCycle` = the node sits on a red edge (derived from `edges`). */
  nodeProps?: (id: string, onCycle: boolean) => WaveGraphNodeProps
}) {
  const layout = useMemo(() => {
    const at = new Map<string, { x: number; y: number }>()
    const byId = new Map<string, WaveGraphNode>()
    for (const node of nodes) {
      byId.set(node.id, node)
      at.set(node.id, {
        x: node.wave * (nodeWidth + waveGap),
        y: node.lane * (nodeHeight + laneGap),
      })
    }
    const onCycle = new Set<string>()
    for (const edge of edges) {
      if (edge.style !== `cycle`) continue
      onCycle.add(edge.from)
      onCycle.add(edge.to)
    }
    return {
      at,
      byId,
      onCycle,
      ...waveGraphSize(nodes, { nodeWidth, nodeHeight, waveGap, laneGap }),
    }
  }, [nodes, edges, nodeWidth, nodeHeight, waveGap, laneGap])
  const out = edgeOut ?? { x: nodeWidth, y: nodeHeight / 2 }
  const into = edgeIn ?? { x: 0, y: nodeHeight / 2 }

  return (
    <div
      className="relative"
      style={{ width: layout.width, height: layout.height }}
    >
      <svg
        aria-hidden
        focusable="false"
        className="pointer-events-none absolute inset-0 overflow-visible"
        width={layout.width}
        height={layout.height}
      >
        {edges.map((edge) => {
          const fromAt = layout.at.get(edge.from)
          const toAt = layout.at.get(edge.to)
          const from = layout.byId.get(edge.from)
          const to = layout.byId.get(edge.to)
          if (!fromAt || !toAt || !from || !to) return null
          const geometry: WaveGraphEdgeGeometry = {
            from,
            to,
            fromAt,
            toAt,
            nodeWidth,
            nodeHeight,
            waveGap,
            laneGap,
          }
          const d = pathFor
            ? pathFor(edge, geometry)
            : defaultEdgePath(geometry, out, into)
          return (
            <path
              key={`${edge.from}:${edge.to}`}
              d={d}
              fill="none"
              strokeWidth={edgeStrokeWidth}
              stroke={EDGE_STROKE[edge.style]}
              strokeDasharray={
                edge.style === `speculative` ? `4 3` : undefined
              }
              data-style={edge.style}
              data-testid={
                edge.style === `plain`
                  ? `${idPrefix}-edge`
                  : `${idPrefix}-${edge.style}-edge`
              }
            />
          )
        })}
      </svg>
      {nodes.map((node) => {
        const position = layout.at.get(node.id)
        const body = renderNode(node.id)
        if (!position || body === null) return null
        const chrome = nodeProps?.(node.id, layout.onCycle.has(node.id)) ?? {}
        return (
          <div
            key={node.id}
            className={cn(
              `absolute flex items-center rounded-md`,
              !chrome.overflowVisible && `overflow-hidden`,
              chrome.className
            )}
            style={{
              left: position.x,
              top: position.y,
              width: nodeWidth,
              height: nodeHeight,
            }}
            data-testid={chrome.testId ?? `${idPrefix}-node-${node.id}`}
            data-wave={node.wave}
            data-lane={node.lane}
          >
            {body}
          </div>
        )
      })}
    </div>
  )
}
