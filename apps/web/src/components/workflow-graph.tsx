import { useMemo, type ReactNode } from "react"
import {
  conceptIcon,
  LiveDot,
  WorkflowGraphView,
  type StatusGlyphProps,
  type WorkflowGraphNode,
  type WorkflowGraphTone,
} from "@exp/ui"
import { RunningIndicator } from "@/components/agent-session-row"
import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { SessionDisplayState } from "@/lib/coding-session-display"
import type { WorkflowNodeRun } from "@/lib/workflow-run"
import type { Issue, WorkflowNode } from "@/db/schema"
import {
  NODE_UNSYNCED_TITLE,
  workflowEdges,
  workflowEdgeStyle,
  workflowNodeCaption,
  workflowNodeTitle,
  workflowNodeTone,
  type EdgeRelation,
} from "@/lib/workflow-view"

// EXP-981: the workflow GRAPH — the wave grid fed by NODE rows instead of
// issues. The geometry is the server's (`wave` = column, `lane` = row,
// `on_cycle` = red); the edges come from the team's synced `blocks` relations
// through `workflowEdges`, so nothing about the DAG is stored twice.
//
// EXP-1033: this file is the BINDING and nothing else — it resolves the
// issues, the live runs and the edge styles and hands them to `@exp/ui`'s
// `WorkflowGraphView`, which draws every node as the app's established ISSUE
// CHIP (a stacked one for a compound node) and scales the whole picture down
// to the column rather than hiding half of it behind an inner scrollbar.
//
// EXP-982: a started workflow's states are real, so a node also wears a state
// GLYPH (the state reads by shape as well as by colour), and once everything
// is in, one extra chip after the last wave stands for the workflow's single
// final pull request.
//
// EXP-1014: ONE rule for the chip's glyph slot ×4 — a live run shows the live
// dot, a started workflow's state shows its own glyph WHEN it has one, and
// everything else (a draft, or blocked / ready / proposed / skipped) shows the
// ISSUE's own status glyph, resolved against the team's rows exactly the way
// `components/issue-chip.tsx` resolves it. An issue that has not synced leaves
// the slot empty.
//
// EXP-983: what an edge says is `workflowEdgeStyle`'s call — green out of a
// landed node, red while the dependent merges a moved upstream in, and DASHED
// while it builds on work that has not landed.
//
// EXP-984: a `proposed` node — a follow-up filed mid-run that nobody admitted
// yet — wears a DASHED outline. It is in the picture, but it is not part of
// the run until a member admits it.

const WarningIcon = conceptIcon(`ui-warning`)
const MergedIcon = conceptIcon(`notification-pr-merged`)
const ErrorIcon = conceptIcon(`ui-error`)
const ReviewIcon = conceptIcon(`nav-reviews`)

/** A live run paints the chip in its SESSION's tone, not the node's. */
const RUN_TONE: Record<SessionDisplayState, WorkflowGraphTone> = {
  running: `success`,
  review: `success`,
  needs_input: `amber`,
  done: `active`,
}

/** The state's shape, beside the caption that carries its colour — for the
 *  states that HAVE one. A state with nothing happening yet (blocked, ready,
 *  proposed) and `skipped` return null, and the chip falls back to the ISSUE's
 *  own status glyph, so an unstarted node reads exactly like the same issue
 *  anywhere else in the app. `running` borrows the session dot. */
export function workflowStateGlyph(state: string): ReactNode | null {
  const className = `size-3.5 shrink-0`
  if (state === `running`) return <LiveDot tone="live" ping className="shrink-0" />
  if (state === `waiting`) return <WarningIcon className={className} />
  if (state === `landed`) return <MergedIcon className={className} />
  if (state === `failed`) return <ErrorIcon className={className} />
  if (state === `in_review` || state === `updating`) {
    return <ReviewIcon className={className} />
  }
  return null
}

export function WorkflowGraph({
  nodes,
  relations,
  issueById,
  workflowStatus,
  cycleEdges,
  finalPr,
  runByNodeId,
  selectedNodeId,
  onSelect,
  className,
}: {
  nodes: readonly WorkflowNode[]
  /** The team's synced relation rows; only `blocks` between covered issues
   *  becomes an edge. */
  relations: readonly EdgeRelation[]
  issueById: ReadonlyMap<string, Issue>
  /** `draft` captions say nothing, a started one says the state. */
  workflowStatus: string
  /** `metrics.cycleEdges` — the edges drawn red. */
  cycleEdges: readonly string[]
  /** EXP-982: `workflowFinalPrCaption`, the workflow's `finalPrUrl` and the
   *  host's own control beside them (the Merge button). A null caption draws
   *  no final chip at all. */
  finalPr?: { caption: string | null; url: string | null; trailing?: ReactNode }
  /** Each node's synced coding session; absent = none (or not synced yet). */
  runByNodeId?: ReadonlyMap<string, WorkflowNodeRun>
  selectedNodeId: string | null
  onSelect: (nodeId: string) => void
  className?: string
}) {
  const { resolve } = useTeamStatusesContext()
  const resolveStatus = (issue: Issue): StatusGlyphProps => {
    const status = resolve(issue)
    return {
      icon: status.icon,
      colorClass: statusColorClass(status),
      colorHex: status.builtinKey ? undefined : status.colorHex,
    }
  }
  const nodeById = useMemo(
    () => new Map(nodes.map((node) => [node.id, node])),
    [nodes]
  )
  // EXP-983: the nodes carry their engine-written serialization edges
  // (`afterNodeIds`), so `workflowEdges` returns those beside the `blocks`
  // ones, and each edge's style is read off the two states it runs between.
  const edges = useMemo(
    () =>
      workflowEdges(nodes, relations, cycleEdges).map((edge) => ({
        from: edge.from,
        to: edge.to,
        style: workflowEdgeStyle(
          edge,
          nodeById.get(edge.from)?.state ?? ``,
          nodeById.get(edge.to)?.state ?? ``
        ),
      })),
    [nodes, relations, cycleEdges, nodeById]
  )

  const chips: WorkflowGraphNode[] = nodes.map((node) => {
    const issue = issueById.get(node.issueId)
    const run = runByNodeId?.get(node.id)
    const liveRun = run?.live ? run : undefined
    // A draft has no states yet, and a started workflow's quiet states
    // (blocked/ready/proposed/skipped) have no glyph of their own: both fall
    // through to the issue's status.
    const stateGlyph =
      workflowStatus === `draft` ? null : workflowStateGlyph(node.state)
    const status = issue ? resolveStatus(issue) : undefined
    return {
      id: node.id,
      wave: node.wave,
      lane: node.lane,
      // A node whose issue row has not synced keeps its place and its caption
      // — the graph must never blank out on a row that is still on its way.
      // EXP-1014: it reads as the issue id's first 8 characters plus the ONE
      // line that says why there is no title yet.
      title: workflowNodeTitle(
        issue?.identifier ?? node.issueId.slice(0, 8),
        node.memberIssueIds.length
      ),
      name: issue?.title ?? NODE_UNSYNCED_TITLE,
      caption: workflowNodeCaption(node, workflowStatus),
      tone: liveRun ? RUN_TONE[liveRun.state] : workflowNodeTone(node.state),
      glyph: liveRun ? (
        <RunningIndicator state={liveRun.state} working={liveRun.working} />
      ) : (
        (stateGlyph ?? undefined)
      ),
      status,
      stacked: node.memberIssueIds.length > 0,
      selected: selectedNodeId === node.id,
      proposed: node.state === `proposed`,
      onCycle: node.onCycle,
      running: Boolean(liveRun),
    }
  })

  const finalPrCaption = finalPr?.caption ?? null

  return (
    <WorkflowGraphView
      nodes={chips}
      edges={edges}
      finalPr={
        finalPrCaption
          ? {
              caption: finalPrCaption,
              url: finalPr?.url ?? null,
              trailing: finalPr?.trailing,
            }
          : undefined
      }
      onSelect={onSelect}
      className={className}
    />
  )
}
