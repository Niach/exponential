import { useMemo } from "react"
import { conceptIcon, LiveDot } from "@exp/ui"
import { RunningIndicator } from "@/components/agent-session-row"
import type { SessionDisplayState } from "@/lib/coding-session-display"
import type { WorkflowNodeRun } from "@/lib/workflow-run"
import type { Issue, WorkflowNode } from "@/db/schema"
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
// stored twice. A node is a CIRCLE in the ring its state earns, its title and
// ONE caption centred underneath, so the edges run circle to circle and never
// through a label. A compound node (a parent run as one batch with its
// sub-issues) is STACKED: a second circle edge peeking out behind the front
// one. While a node's run is up the circle carries that session's own dot
// (`RunningIndicator`), pinging only while the agent works.
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

const NODE_CIRCLE = 30
const NODE_W = 140
const NODE_H = NODE_CIRCLE + 4 + 16 + 16
const WAVE_GAP = 64
const LANE_GAP = 14
/** The air an edge keeps from the circle it leaves or enters. */
const EDGE_AIR = 4
const EDGE_OUT = { x: NODE_W / 2 + NODE_CIRCLE / 2 + EDGE_AIR, y: NODE_CIRCLE / 2 }
const EDGE_IN = { x: NODE_W / 2 - NODE_CIRCLE / 2 - EDGE_AIR, y: NODE_CIRCLE / 2 }

/** The caption's paint per `workflowNodeTone` (amber = a person is needed). */
const TONE_CLASS: Record<WorkflowNodeTone, string> = {
  muted: `text-muted-foreground`,
  active: `text-primary`,
  amber: `text-amber-500`,
  success: `text-emerald-500`,
  danger: `text-destructive`,
}

/** The circle's ring + wash per tone. A quiet node keeps the glass hairline. */
const RING_CLASS: Record<WorkflowNodeTone, string> = {
  muted: `border-glass-stroke-strong bg-glass-row`,
  active: `border-primary bg-primary/15`,
  amber: `border-amber-500 bg-amber-500/15`,
  success: `border-emerald-500 bg-emerald-500/15`,
  danger: `border-destructive bg-destructive/15`,
}

/** A live run paints the circle in its SESSION's tone, not the node's. */
const RUN_RING_CLASS: Record<SessionDisplayState, string> = {
  running: `border-emerald-500 bg-emerald-500/15`,
  review: `border-emerald-500 bg-emerald-500/15`,
  needs_input: `border-amber-500 bg-amber-500/15`,
  done: `border-sky-500 bg-sky-500/15`,
}
const RUN_TEXT_CLASS: Record<SessionDisplayState, string> = {
  running: `text-emerald-500`,
  review: `text-emerald-500`,
  needs_input: `text-amber-500`,
  done: `text-sky-500`,
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
  /** `draft` captions name the PLAN, a started one names the state. */
  workflowStatus: string
  /** `metrics.cycleEdges` — the edges drawn red. */
  cycleEdges: readonly string[]
  /** EXP-982: `workflowFinalPrCaption` and the workflow's `finalPrUrl`. A null
   *  caption draws no final node at all. */
  finalPr?: { caption: string | null; url: string | null }
  /** Each node's synced coding session; absent = none (or not synced yet). */
  runByNodeId?: ReadonlyMap<string, WorkflowNodeRun>
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
    <div className={cn(`overflow-auto p-1.5`, className)} data-testid="workflow-graph">
      <WaveGraph
        nodes={gridNodes}
        edges={edges}
        nodeWidth={NODE_W}
        nodeHeight={NODE_H}
        waveGap={WAVE_GAP}
        laneGap={LANE_GAP}
        edgeOut={EDGE_OUT}
        edgeIn={EDGE_IN}
        idPrefix="workflow-graph"
        nodeProps={(id) => ({
          // The cell itself stays plain: the circle inside carries the ring,
          // and its ping halo and pick ring paint past the cell's box.
          className: `items-stretch`,
          overflowVisible: true,
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
              run={runByNodeId?.get(node.id)}
              selected={selectedNodeId === node.id}
              onSelect={() => onSelect(node.id)}
            />
          )
        }}
      />
    </div>
  )
}

/** One node: the circle over its title and caption. A node whose issue row
 *  has not synced keeps its caption — the graph must never blank out on a row
 *  that is still on its way. */
export function WorkflowNodeCard({
  node,
  issue,
  workflowStatus,
  run,
  selected,
  onSelect,
}: {
  node: WorkflowNode
  issue: Issue | undefined
  workflowStatus: string
  run?: WorkflowNodeRun
  selected: boolean
  onSelect: () => void
}) {
  const compound = node.memberIssueIds.length > 0
  const title = issue
    ? workflowNodeTitle(issue.identifier, node.memberIssueIds.length)
    : node.issueId.slice(0, 8)
  const caption = workflowNodeCaption(node, workflowStatus)
  const tone = workflowNodeTone(node.state)
  const liveRun = run?.live ? run : undefined
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      data-testid={`workflow-node-${node.id}-card`}
      data-running={liveRun ? `true` : undefined}
      className="group flex h-full w-full cursor-pointer flex-col items-center gap-1 rounded-md outline-none"
    >
      <span
        className="relative shrink-0"
        style={{ width: NODE_CIRCLE, height: NODE_CIRCLE }}
      >
        {compound && (
          /* The batch's second circle edge, peeking out top-right. */
          <span
            aria-hidden
            className="absolute -top-[3px] left-1 size-full rounded-full border border-glass-stroke-strong"
            data-testid={`workflow-node-${node.id}-stack`}
          />
        )}
        <span
          data-testid={`workflow-node-${node.id}-circle`}
          className={cn(
            `absolute inset-0 flex items-center justify-center rounded-full border transition-colors duration-fast group-hover:brightness-125 group-focus-visible:ring-[3px] group-focus-visible:ring-ring/50`,
            liveRun ? RUN_RING_CLASS[liveRun.state] : RING_CLASS[tone],
            liveRun ? RUN_TEXT_CLASS[liveRun.state] : TONE_CLASS[tone],
            // Not admitted into the run yet — the same dashed outline the
            // final pull request wears while it is still only an intention.
            node.state === `proposed` && `border-dashed`,
            // The pick is a halo OUTSIDE the ring, so the state colour stays.
            selected && `outline outline-1 outline-offset-2 outline-primary`,
            node.onCycle && `border-destructive`
          )}
        >
          {liveRun ? (
            <RunningIndicator state={liveRun.state} working={liveRun.working} />
          ) : (
            /* A draft's caption names the plan, so no state reads off it. */
            workflowStatus !== `draft` && <WorkflowStateGlyph state={node.state} />
          )}
        </span>
      </span>
      <span className="flex w-full min-w-0 flex-col items-center">
        <span
          className={cn(
            `max-w-full truncate text-xs`,
            selected && `font-medium`,
            !issue && `font-mono text-muted-foreground`
          )}
          data-testid={`workflow-node-${node.id}-title`}
        >
          {title}
        </span>
        <span
          className={cn(
            `max-w-full truncate text-xs`,
            liveRun ? RUN_TEXT_CLASS[liveRun.state] : TONE_CLASS[tone]
          )}
          data-testid={`workflow-node-${node.id}-caption`}
        >
          {caption}
        </span>
      </span>
    </button>
  )
}

/** The ONE final pull request, integration branch → default branch. It is not
 *  a workflow node: nothing selects it, and it links out to GitHub once the
 *  engine opened it. */
function FinalPrCard({ caption, url }: { caption: string; url: string | null }) {
  const body = (
    <>
      <span
        className="flex shrink-0 items-center justify-center rounded-full border border-dashed border-glass-stroke-strong bg-glass-row text-muted-foreground"
        style={{ width: NODE_CIRCLE, height: NODE_CIRCLE }}
      >
        <MergedIcon className="size-3.5" />
      </span>
      <span className="flex w-full min-w-0 flex-col items-center">
        <span className="max-w-full truncate text-xs">{FINAL_PR_TITLE}</span>
        <span className="max-w-full truncate text-xs text-muted-foreground">
          {caption}
        </span>
      </span>
    </>
  )
  const className = `flex h-full w-full flex-col items-center gap-1 rounded-md`
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
        `outline-none transition-colors duration-fast hover:brightness-125 focus-visible:ring-[3px] focus-visible:ring-ring/50`
      )}
    >
      {body}
    </a>
  )
}
