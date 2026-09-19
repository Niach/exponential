import { useMemo } from "react"
import { conceptIcon, LiveDot } from "@exp/ui"
import type { Issue, WorkflowNode } from "@/db/schema"
import { IssueChip } from "@/components/issue-chip"
import { WaveGraph } from "@/components/issue-graph"
import {
  workflowEdges,
  workflowEdgeStyle,
  workflowNodeCaption,
  workflowNodeTitle,
  workflowNodeTone,
  FINAL_PR_TITLE,
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
//
// EXP-982: a started workflow's states are real, so a node also wears a state
// GLYPH (the state reads by shape as well as by colour), and once everything
// is in, one extra node after the last wave stands for the workflow's single
// final pull request.
//
// EXP-983: what an edge says is `workflowEdgeStyle`'s call — green out of a
// landed node, red while the dependent merges a moved upstream in, and DASHED
// while it builds on work that has not landed (a speculative start, or a
// serialization edge two colliding siblings were given).
//
// EXP-984: a `proposed` node — a follow-up filed mid-run that nobody admitted
// yet — is drawn with a DASHED outline. It is in the picture, but it is not
// part of the run until a member admits it.

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

const WarningIcon = conceptIcon(`ui-warning`)
const MergedIcon = conceptIcon(`notification-pr-merged`)
const ErrorIcon = conceptIcon(`ui-error`)
const ReviewIcon = conceptIcon(`nav-reviews`)

/** The final-PR node's id — never a uuid, so it can never collide with one. */
const FINAL_PR_NODE_ID = `final-pr`

/** The state's shape, beside the caption that carries its colour. States with
 *  nothing happening yet (proposed/blocked/ready/paused) and `skipped` stay
 *  bare: the caption alone says it. `running` borrows the session dot. */
export function WorkflowStateGlyph({ state }: { state: string }) {
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
  /** EXP-982: `workflowFinalPrCaption` and the workflow's `finalPrUrl`. A null
   *  caption draws no final node at all. */
  finalPr?: { caption: string | null; url: string | null }
  selectedNodeId: string | null
  onSelect: (nodeId: string) => void
  className?: string
}) {
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
  // The final PR sits one wave past everything else, in lane 0. No edge runs
  // into it: it is the whole graph's outcome, not one node's dependent.
  const finalPrCaption = finalPr?.caption ?? null
  const gridNodes = useMemo(() => {
    if (!finalPrCaption) return nodes
    const lastWave = nodes.reduce((max, node) => Math.max(max, node.wave), 0)
    return [
      ...nodes,
      { id: FINAL_PR_NODE_ID, wave: lastWave + 1, lane: 0 },
    ]
  }, [nodes, finalPrCaption])

  if (nodes.length === 0) return null

  return (
    <div className={cn(`overflow-auto`, className)} data-testid="workflow-graph">
      <WaveGraph
        nodes={gridNodes}
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
          if (id === FINAL_PR_NODE_ID) {
            return (
              <FinalPrCard
                caption={finalPrCaption ?? ``}
                url={finalPr?.url ?? null}
              />
            )
          }
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
          // Not admitted into the run yet — the same dashed outline the final
          // pull request wears while it is still only an intention.
          node.state === `proposed` && `border-dashed`,
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
          className={cn(
            `flex min-w-0 items-center gap-1.5 text-xs`,
            TONE_CLASS[workflowNodeTone(node.state)]
          )}
          data-testid={`workflow-node-${node.id}-caption`}
        >
          {/* A draft's caption names the plan, so no state reads off it. */}
          {workflowStatus !== `draft` && <WorkflowStateGlyph state={node.state} />}
          <span className="truncate">{caption}</span>
        </span>
      </button>
    </div>
  )
}

/** The ONE final pull request, integration branch → default branch. It is not
 *  a workflow node: nothing selects it, and it links out to GitHub once the
 *  engine opened it. */
function FinalPrCard({ caption, url }: { caption: string; url: string | null }) {
  const body = (
    <>
      <span className="truncate text-sm font-medium">{FINAL_PR_TITLE}</span>
      <span className="truncate text-xs text-muted-foreground">{caption}</span>
    </>
  )
  const className = `flex h-full w-full flex-col justify-center gap-1 rounded-md border border-dashed border-glass-stroke bg-glass-row px-2 py-1 text-left`
  if (!url) {
    return (
      <div className={className} data-testid="workflow-final-pr">
        {body}
      </div>
    )
  }
  return (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      data-testid="workflow-final-pr"
      className={cn(
        className,
        `outline-none transition-colors duration-fast hover:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50`
      )}
    >
      {body}
    </a>
  )
}
