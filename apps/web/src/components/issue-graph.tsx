import { useMemo, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Board, Issue, Team } from "@/db/schema"
import { boardCollection, teamCollection } from "@/lib/collections"
import { useTeamIssueGraph } from "@/hooks/use-team-issue-graph"
import {
  blockGraph,
  ISSUE_GRAPH_CYCLE_NOTE,
  ISSUE_GRAPH_GEOMETRY,
  issueGraphSize,
  ISSUE_GRAPH_TRUNCATED_NOTE,
  type IssueGraph,
} from "@/lib/issue-graph"
import { WaveGraph, type WorkflowEdgeStyle } from "@exp/ui"
import { IssueChip } from "@/components/issue-chip"
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
// EXP-981/EXP-1033: the drawing half is `WaveGraph`, and it lives in `@exp/ui`
// (`packages/ui/src/wave-graph.tsx`) — the workflow graph feeds it nodes keyed
// by NODE id with a chip renderer of its own (`@exp/ui` `WorkflowGraphView`),
// the issue flavour below keeps feeding it issues.
//
// EXP-983: every edge arrives with its STYLE (`lib/workflow-view.ts`
// `workflowEdgeStyle`) rather than a flag per meaning — grey solid, red on a
// cycle or a stale upstream, green out of a landed node, grey DASHED while the
// dependent builds on work nobody landed yet.
//
// EXP-1057: the geometry is THE one of `issue-graph-geometry.json` (identical
// ×4). The grid sits `inset` inside its scroll box, so the rings on the edge
// columns are never clipped, and the box is sized from the grid, capped at
// the contract's viewport.

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

  const g = ISSUE_GRAPH_GEOMETRY
  const size = issueGraphSize(
    Math.max(...graph.nodes.map((node) => node.wave)) + 1,
    Math.max(...graph.nodes.map((node) => node.lane)) + 1
  )
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
      <div
        className="overflow-auto"
        style={{ maxWidth: size.viewWidth, maxHeight: size.viewHeight }}
        data-testid="issue-graph-viewport"
      >
        <div style={{ padding: g.inset, width: size.width, height: size.height }}>
        <WaveGraph
          nodes={graph.nodes}
          edges={edges}
          nodeWidth={g.nodeWidth}
          nodeHeight={g.nodeHeight}
          waveGap={g.waveGap}
          laneGap={g.laneGap}
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
