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
import { cn } from "@/lib/utils"

// EXP-980: THE blocks mini-graph. One component behind the list's blocks
// badge, the PrGraphBadge overlay's Issue face and the blocked-start dialog
// (and, from P2 on, the workflow wave graph: the layout then comes from the
// server, the drawing stays this).
//
// No graph library: `lib/issue-graph.ts` hands every node a `wave` (column)
// and a `lane` (row), so this is a grid of chips with one SVG of edges behind
// it. An edge runs from its blocker's right edge to the blocked issue's left
// edge: grey, RED on a blocking cycle. Subjects wear the accent ring, cycle
// members a red one. Phones get the same rows as a list grouped by wave
// (`IssueGraphList` on the natives); the web keeps the grid and scrolls it.

const NODE_W = 116
const NODE_H = 28
const WAVE_GAP = 40
const LANE_GAP = 8

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
  const layout = useMemo(() => {
    const at = new Map<string, { x: number; y: number }>()
    let waves = 0
    let lanes = 0
    for (const node of graph.nodes) {
      at.set(node.id, {
        x: node.wave * (NODE_W + WAVE_GAP),
        y: node.lane * (NODE_H + LANE_GAP),
      })
      waves = Math.max(waves, node.wave + 1)
      lanes = Math.max(lanes, node.lane + 1)
    }
    const onCycle = new Set<string>()
    for (const edge of graph.edges) {
      if (!edge.cycle) continue
      onCycle.add(edge.from)
      onCycle.add(edge.to)
    }
    return {
      at,
      onCycle,
      width: Math.max(0, waves * (NODE_W + WAVE_GAP) - WAVE_GAP),
      height: Math.max(0, lanes * (NODE_H + LANE_GAP) - LANE_GAP),
    }
  }, [graph])

  if (graph.nodes.length === 0) return null

  return (
    <div className={cn(`flex flex-col gap-2`, className)} data-testid="issue-graph">
      <div className="max-h-72 overflow-auto">
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
            {graph.edges.map((edge) => {
              const from = layout.at.get(edge.from)
              const to = layout.at.get(edge.to)
              if (!from || !to) return null
              const x1 = from.x + NODE_W
              const y1 = from.y + NODE_H / 2
              const x2 = to.x
              const y2 = to.y + NODE_H / 2
              // A forward edge bows half the gap; a cycle edge that runs
              // backwards (or inside one wave) still reads as a curve.
              const bend = Math.max(WAVE_GAP / 2, Math.abs(x2 - x1) / 2)
              return (
                <path
                  key={`${edge.from}:${edge.to}`}
                  d={`M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`}
                  fill="none"
                  strokeWidth={1.25}
                  stroke={
                    edge.cycle
                      ? `var(--destructive)`
                      : `var(--glass-stroke-strong)`
                  }
                  data-testid={edge.cycle ? `issue-graph-cycle-edge` : `issue-graph-edge`}
                />
              )
            })}
          </svg>
          {graph.nodes.map((node) => {
            const issue = issueById.get(node.id)
            const position = layout.at.get(node.id)
            if (!issue || !position) return null
            return (
              <div
                key={node.id}
                className={cn(
                  `absolute flex items-center overflow-hidden rounded-md`,
                  node.subject && `ring-1 ring-primary`,
                  layout.onCycle.has(node.id) && `ring-1 ring-destructive`
                )}
                style={{
                  left: position.x,
                  top: position.y,
                  width: NODE_W,
                  height: NODE_H,
                }}
                data-testid={`issue-graph-node-${issue.identifier}`}
                data-wave={node.wave}
                data-lane={node.lane}
              >
                {renderNode(issue)}
              </div>
            )
          })}
        </div>
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
            preview={false}
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
