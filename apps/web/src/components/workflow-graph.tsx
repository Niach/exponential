import { useMemo } from "react"
import type { Issue, WorkflowNode } from "@/db/schema"
import { IssueChip } from "@/components/issue-chip"
import { WaveGraph } from "@/components/issue-graph"
import {
  workflowEdges,
  workflowNodeCaption,
  workflowNodeTitle,
  workflowNodeTone,
  type EdgeRelation,
  type WorkflowNodeTone,
} from "@/lib/workflow-view"
import { cn } from "@/lib/utils"

// EXP-981: the workflow GRAPH — the EXP-980 wave grid (`WaveGraph`) fed by
// NODE rows instead of issues. The geometry is the server's (`wave` = column,
// `lane` = row, `on_cycle` = red); the edges come from the team's synced
// `blocks` relations through `workflowEdges`, so nothing about the DAG is
// stored twice. A compound node (a parent run as one batch with its
// sub-issues) is drawn as a STACKED card: a second card edge peeking out
// behind the front one.
//
// Phones keep the grid and scroll it, exactly like the blocks mini-graph —
// the wave-grouped list is the natives' phone form (`IssueGraphList`).

const NODE_W = 208
const NODE_H = 66

/** The caption's paint per `workflowNodeTone` (amber = a person is needed). */
const TONE_CLASS: Record<WorkflowNodeTone, string> = {
  muted: `text-muted-foreground`,
  active: `text-primary`,
  amber: `text-amber-500`,
  success: `text-emerald-500`,
  danger: `text-destructive`,
}

export function WorkflowGraph({
  nodes,
  relations,
  issueById,
  workflowStatus,
  cycleEdges,
  selectedNodeId,
  onSelect,
  className,
}: {
  nodes: readonly WorkflowNode[]
  /** The team's synced relation rows; only `blocks` between covered issues
   *  becomes an edge. */
  relations: readonly EdgeRelation[]
  issueById: ReadonlyMap<string, Issue>
  /** `draft` captions name the PLAN, a started one names the state. */
  workflowStatus: string
  /** `metrics.cycleEdges` — the edges drawn red. */
  cycleEdges: readonly string[]
  selectedNodeId: string | null
  onSelect: (nodeId: string) => void
  className?: string
}) {
  const edges = useMemo(
    () => workflowEdges(nodes, relations, cycleEdges),
    [nodes, relations, cycleEdges]
  )
  const nodeById = useMemo(
    () => new Map(nodes.map((node) => [node.id, node])),
    [nodes]
  )

  if (nodes.length === 0) return null

  return (
    <div className={cn(`overflow-auto`, className)} data-testid="workflow-graph">
      <WaveGraph
        nodes={nodes}
        edges={edges}
        nodeWidth={NODE_W}
        nodeHeight={NODE_H}
        idPrefix="workflow-graph"
        nodeProps={(id) => ({
          // The box itself stays plain: the card inside carries the ring, so
          // the stacked edge behind it is not ringed twice.
          className: `items-stretch`,
          testId: `workflow-node-${id}`,
        })}
        renderNode={(id) => {
          const node = nodeById.get(id)
          if (!node) return null
          return (
            <WorkflowNodeCard
              node={node}
              issue={issueById.get(node.issueId)}
              workflowStatus={workflowStatus}
              selected={selectedNodeId === node.id}
              onSelect={() => onSelect(node.id)}
            />
          )
        }}
      />
    </div>
  )
}

/** One node's card. A node whose issue row has not synced keeps its caption —
 *  the graph must never blank out on a row that is still on its way. */
export function WorkflowNodeCard({
  node,
  issue,
  workflowStatus,
  selected,
  onSelect,
}: {
  node: WorkflowNode
  issue: Issue | undefined
  workflowStatus: string
  selected: boolean
  onSelect: () => void
}) {
  const compound = node.memberIssueIds.length > 0
  const title = issue
    ? workflowNodeTitle(issue.identifier, node.memberIssueIds.length)
    : null
  const caption = workflowNodeCaption(node, workflowStatus)
  return (
    <div className="relative h-full w-full">
      {compound && (
        /* The batch's second card edge, peeking out top-right. */
        <div
          aria-hidden
          className="absolute top-0 right-0 h-[calc(100%-4px)] w-[calc(100%-4px)] rounded-md border border-glass-stroke bg-glass-section"
          data-testid={`workflow-node-${node.id}-stack`}
        />
      )}
      <button
        type="button"
        onClick={onSelect}
        aria-pressed={selected}
        data-testid={`workflow-node-${node.id}-card`}
        className={cn(
          `absolute bottom-0 left-0 flex h-[calc(100%-4px)] w-[calc(100%-4px)] cursor-pointer flex-col justify-center gap-1 rounded-md border border-glass-stroke bg-glass-row px-2 py-1 text-left outline-none transition-colors duration-fast hover:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50`,
          selected && `ring-1 ring-primary`,
          node.onCycle && `ring-1 ring-destructive`
        )}
      >
        {issue && title ? (
          <IssueChip
            issue={{ ...issue, identifier: title }}
            preview={false}
            className="w-full border-0 bg-transparent px-0"
          />
        ) : (
          <span className="truncate font-mono text-xs text-muted-foreground">
            {node.issueId.slice(0, 8)}
          </span>
        )}
        <span
          className={cn(`truncate text-xs`, TONE_CLASS[workflowNodeTone(node.state)])}
          data-testid={`workflow-node-${node.id}-caption`}
        >
          {caption}
        </span>
      </button>
    </div>
  )
}
