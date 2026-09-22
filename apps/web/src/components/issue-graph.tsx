import { useMemo, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Board, Issue, Team } from "@/db/schema"
import { boardCollection, teamCollection } from "@/lib/collections"
import { useTeamIssueGraph } from "@/hooks/use-team-issue-graph"
import {
  blockGraph,
  ISSUE_GRAPH_CYCLE_NOTE,
  ISSUE_GRAPH_TRUNCATED_NOTE,
  type IssueGraph,
} from "@/lib/issue-graph"
import { IssueChip } from "@/components/issue-chip"
import type { WorkflowEdgeStyle } from "@/lib/workflow-view"
import { cn } from "@/lib/utils"

// EXP-980: THE blocks mini-graph. One component behind the list's blocks
// badge, the PrGraphBadge overlay's Issue face and the blocked-start dialog.
//
// No graph library: every node arrives with a `wave` (column) and a `lane`
// (row) — derived here by `lib/issue-graph.ts`, synced from the server for a
// workflow (EXP-981) — so this is a grid of boxes with one SVG of edges
// behind it. An edge runs from its blocker's right edge to the blocked node's
// left edge: grey, RED on a blocking cycle. Subjects wear the accent ring,
// cycle members a red one. Phones get the same rows as a list grouped by wave
// (`IssueGraphList` on the natives); the web keeps the grid and scrolls it.
//
// EXP-981: the drawing half is `WaveGraph` — the workflow graph feeds it nodes
// keyed by NODE id with a card renderer of its own
// (`components/workflow-graph.tsx`), the issue flavour below keeps feeding it
// issues.
//
// EXP-983: every edge arrives with its STYLE (`lib/workflow-view.ts`
// `workflowEdgeStyle`) rather than a flag per meaning — grey solid, red on a
// cycle or a stale upstream, green out of a landed node, grey DASHED while the
// dependent builds on work nobody landed yet.

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
   *  graph through `workflowEdgeStyle`, the issue flavour below with nothing
   *  but `plain` and `cycle` (a blocking cycle) to say. */
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
  /** The cell paints outside its box (a ping halo, a pick ring). */
  overflowVisible?: boolean
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
   *  Boxes default to their side middles; the workflow graph's circles
   *  anchor on the circle, not on the label underneath it. */
  edgeOut?: { x: number; y: number }
  edgeIn?: { x: number; y: number }
  /** Names the edge paths and any unnamed node box in the DOM. */
  idPrefix?: string
  renderNode: (id: string) => ReactNode
  /** `onCycle` = the node sits on a red edge (derived from `edges`). */
  nodeProps?: (id: string, onCycle: boolean) => WaveGraphNodeProps
}) {
  const layout = useMemo(() => {
    const at = new Map<string, { x: number; y: number }>()
    let waves = 0
    let lanes = 0
    for (const node of nodes) {
      at.set(node.id, {
        x: node.wave * (nodeWidth + waveGap),
        y: node.lane * (nodeHeight + laneGap),
      })
      waves = Math.max(waves, node.wave + 1)
      lanes = Math.max(lanes, node.lane + 1)
    }
    const onCycle = new Set<string>()
    for (const edge of edges) {
      if (edge.style !== `cycle`) continue
      onCycle.add(edge.from)
      onCycle.add(edge.to)
    }
    return {
      at,
      onCycle,
      width: Math.max(0, waves * (nodeWidth + waveGap) - waveGap),
      height: Math.max(0, lanes * (nodeHeight + laneGap) - laneGap),
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
          const from = layout.at.get(edge.from)
          const to = layout.at.get(edge.to)
          if (!from || !to) return null
          const x1 = from.x + out.x
          const y1 = from.y + out.y
          const x2 = to.x + into.x
          const y2 = to.y + into.y
          // The curve lives in the GAP between two cells: a level stub runs
          // from the anchor to its cell's edge first, so an edge never cuts
          // through a label that sits beside (or under) its anchor. A cycle
          // edge that runs backwards (or inside one wave) has no gap to
          // curve in and bows half a gap instead.
          const gapStart = from.x + nodeWidth
          const gapEnd = to.x
          const forward = gapEnd > gapStart
          const c1 = forward ? gapStart : x1
          const c2 = forward ? gapEnd : x2
          const bend = forward
            ? (c2 - c1) / 2
            : Math.max(waveGap / 2, Math.abs(x2 - x1) / 2)
          const d = `M ${x1} ${y1} L ${c1} ${y1} C ${c1 + bend} ${y1}, ${c2 - bend} ${y2}, ${c2} ${y2} L ${x2} ${y2}`
          return (
            <path
              key={`${edge.from}:${edge.to}`}
              d={d}
              fill="none"
              strokeWidth={1.25}
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

export function IssueGraphView({
  graph,
  issueById,
  renderNode,
  className,
}: {
  graph: IssueGraph
  issueById: ReadonlyMap<string, Issue>
  /** The node's chip; the caller owns navigation. */
  renderNode: (issue: Issue) => ReactNode
  className?: string
}) {
  if (graph.nodes.length === 0) return null

  const subjects = new Set(
    graph.nodes.filter((node) => node.subject).map((node) => node.id)
  )
  // The issue flavour has no run behind it: an edge is either inside a cycle
  // or it has nothing to say (EXP-983).
  const edges = graph.edges.map((edge) => ({
    from: edge.from,
    to: edge.to,
    style: (edge.cycle ? `cycle` : `plain`) as WorkflowEdgeStyle,
  }))

  return (
    <div className={cn(`flex flex-col gap-2`, className)} data-testid="issue-graph">
      <div className="max-h-72 overflow-auto">
        <WaveGraph
          nodes={graph.nodes}
          edges={edges}
          renderNode={(id) => {
            const issue = issueById.get(id)
            return issue ? renderNode(issue) : null
          }}
          nodeProps={(id, onCycle) => ({
            className: cn(
              subjects.has(id) && `ring-1 ring-primary`,
              onCycle && `ring-1 ring-destructive`
            ),
            testId: `issue-graph-node-${issueById.get(id)?.identifier ?? id}`,
          })}
        />
      </div>
      {graph.hasCycle && (
        <div className="text-xs text-destructive">{ISSUE_GRAPH_CYCLE_NOTE}</div>
      )}
      {graph.truncated && (
        <div className="text-xs text-muted-foreground">
          {ISSUE_GRAPH_TRUNCATED_NOTE}
        </div>
      )}
    </div>
  )
}

/**
 * The graph around `subjectIds`, self-fed from the synced rows: mount it only
 * while it shows (an open popover, an open dialog), never per list row.
 */
export function TeamIssueGraph({
  teamId,
  subjectIds,
  onNavigate,
  empty,
  className,
}: {
  teamId: string
  subjectIds: readonly string[]
  /** A chip was followed: close the surface hosting the graph. */
  onNavigate?: () => void
  /** Rendered when the subjects have no open blocks relation at all. */
  empty?: ReactNode
  className?: string
}) {
  // The GRAPH's team, not the route's: an inbox or My Issues row can sit in
  // another team than the URL.
  const { data: teamRows } = useLiveQuery(
    (query) =>
      query.from({ t: teamCollection }).where(({ t }) => eq(t.id, teamId)),
    [teamId]
  )
  const teamSlug = ((teamRows ?? []) as Team[])[0]?.slug
  const { relations, issues } = useTeamIssueGraph(teamId)
  const { data: boardRows } = useLiveQuery(
    (query) =>
      query.from({ b: boardCollection }).where(({ b }) => eq(b.teamId, teamId)),
    [teamId]
  )
  const boardSlugById = useMemo(
    () => new Map(((boardRows ?? []) as Board[]).map((row) => [row.id, row.slug])),
    [boardRows]
  )
  const issueById = useMemo(
    () => new Map(issues.map((issue) => [issue.id, issue])),
    [issues]
  )
  const subjectKey = subjectIds.join(`,`)
  const graph = useMemo(
    () => blockGraph(subjectKey ? subjectKey.split(`,`) : [], relations, issues),
    [subjectKey, relations, issues]
  )

  if (graph.edges.length === 0) return empty ?? null

  return (
    <IssueGraphView
      graph={graph}
      issueById={issueById}
      className={className}
      renderNode={(issue) => {
        const boardSlug = boardSlugById.get(issue.boardId)
        return (
          <IssueChip
            issue={issue}
            className="w-full"
            link={
              teamSlug && boardSlug
                ? (props) => (
                    <Link
                      to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                      params={{
                        teamSlug,
                        boardSlug,
                        issueIdentifier: issue.identifier,
                      }}
                      onClick={onNavigate}
                      {...props}
                    />
                  )
                : undefined
            }
          />
        )
      }}
    />
  )
}
