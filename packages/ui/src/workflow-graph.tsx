import { useCallback, useMemo, useRef, useState, type ReactNode } from "react"
import { conceptIcon } from "./icons.generated"
import { IssueChipStack } from "./issue-chip"
import { StatusGlyph, type StatusGlyphProps } from "./status-glyph"
import { WaveGraph, waveGraphSize, type WaveGraphEdge } from "./wave-graph"
import { cn } from "./cn"

// EXP-1033 — the workflow GRAPH, presentational half.
//
// EXP-981 drew a node as a CIRCLE with its title and caption underneath, and
// the screen paid for it twice: the labels could not be read at a glance, and
// a graph wider than its column was cut off behind an inner scrollbar. A node
// is the app's established ISSUE CHIP now — the same 6px rect every other
// surface names an issue with — laid out on the same wave grid, with the ONE
// caption as trailing text INSIDE the chip rather than a second line under it.
// A compound node (a parent run as one batch with its sub-issues) is the same
// chip through `IssueChipStack`.
//
// The FILL rule: no inner scrollbar, no fixed height, nothing clipped. The
// grid's natural size is measured (`waveGraphSize`) and the whole picture is
// scaled DOWN to the container's width with one transform — never up, so a
// small graph keeps its true size — and the container takes the scaled
// height. Phones get the same scaled graph; a wide graph simply draws small.
//
// Pure props: no live query, no router, no team. The app's
// `components/workflow-graph.tsx` resolves the issues, the runs and the edges
// and feeds them in; the styleguide island feeds a fixture.

const MergedIcon = conceptIcon(`notification-pr-merged`)

/** The final-PR node's id — never a uuid, so it can never collide with one. */
export const WORKFLOW_FINAL_PR_NODE_ID = `final-pr`

/** The chip that IS a node: wide enough for an identifier, a title and the
 *  state word, exactly one chip high. */
const NODE_W = 200
const NODE_H = 28
const WAVE_GAP = 48
const LANE_GAP = 10
/** The air the scaled picture keeps for a stack's ghosts and a pick ring. */
const GRAPH_PAD = 8

/** The tone a node's state paints in — `lib/workflow-view.ts`'s
 *  `workflowNodeTone`, or a live run's own session tone. */
export type WorkflowGraphTone =
  | `muted`
  | `active`
  | `amber`
  | `success`
  | `danger`

/** The caption's paint per tone (amber = a person is needed). */
const TONE_CLASS: Record<WorkflowGraphTone, string> = {
  muted: `text-muted-foreground`,
  active: `text-primary`,
  amber: `text-amber-500`,
  success: `text-emerald-500`,
  danger: `text-destructive`,
}

export interface WorkflowGraphNode {
  id: string
  /** The column — the server's `wave`. */
  wave: number
  /** The row — the server's `lane`. */
  lane: number
  /** The mono identifier, `EXP-14 +3` for a compound node
   *  (`workflowNodeTitle`). */
  title: string
  /** The issue's own name, truncated inside the chip. */
  name: string
  /** The ONE caption, trailing inside the chip: empty in a draft, the bare
   *  state label once the workflow started. */
  caption: string
  tone: WorkflowGraphTone
  /** The chip's glyph slot — the caller's node: the workflow state glyph (the
   *  slot paints it in `tone`) or the live run's own dot. Absent falls through
   *  to `status`. */
  glyph?: ReactNode
  /** EXP-1014: what the slot falls back to when there is no `glyph` — the
   *  ISSUE's own already-resolved status, so an unstarted node reads exactly
   *  like the same issue on every other surface. Absent = an empty slot (the
   *  issue row has not synced). */
  status?: StatusGlyphProps
  /** A compound node: the chip rides a stack of ghosts. */
  stacked?: boolean
  selected?: boolean
  /** Filed mid-run and not admitted yet: a dashed outline. */
  proposed?: boolean
  /** Inside a blocking cycle: a red outline. */
  onCycle?: boolean
  /** Its run is up right now (the caller's glyph already says how). */
  running?: boolean
}

export interface WorkflowGraphFinalPr {
  /** `#42 · Open`, `Opening the pull request`. */
  caption: string
  url?: string | null
  /** The host's own control beside the caption — the web's Merge button. */
  trailing?: ReactNode
}

/** The final-PR chip's name, byte-identical ×4 (`FINAL_PR_TITLE`). */
const FINAL_PR_TITLE = `Final pull request`

/**
 * The container's inner width, live. Measured from a ref CALLBACK, not an
 * effect: the view renders nothing while its nodes are still on their way
 * (a cold load reads the workflow row one shape before its nodes), so the
 * measured div mounts LATER than the component — an effect with empty deps
 * would have run once against no element and never again, leaving the
 * scale pinned to 1 and a wide graph overflowing its column. SSR is off in
 * this app, so the first paint of the div measures before anything flashes.
 */
function useContainerWidth() {
  const observer = useRef<ResizeObserver | null>(null)
  const [width, setWidth] = useState(0)
  const ref = useCallback((host: HTMLDivElement | null) => {
    observer.current?.disconnect()
    observer.current = null
    if (!host) return
    const measure = () => setWidth(host.clientWidth)
    measure()
    if (typeof ResizeObserver === `undefined`) return
    observer.current = new ResizeObserver(measure)
    observer.current.observe(host)
  }, [])
  return { ref, width }
}

export function WorkflowGraphView({
  nodes,
  edges,
  finalPr,
  onSelect,
  className,
}: {
  nodes: readonly WorkflowGraphNode[]
  edges: readonly WaveGraphEdge[]
  /** EXP-982: one more chip after the last wave, the run's single outcome.
   *  Absent = the run is not there yet. */
  finalPr?: WorkflowGraphFinalPr
  onSelect?: (nodeId: string) => void
  className?: string
}) {
  const nodeById = useMemo(
    () => new Map(nodes.map((node) => [node.id, node])),
    [nodes]
  )
  // The final PR sits one wave past everything else, in lane 0. No edge runs
  // into it: it is the whole graph's outcome, not one node's dependent.
  const cells = useMemo(() => {
    const grid = nodes.map((node) => ({
      id: node.id,
      wave: node.wave,
      lane: node.lane,
    }))
    if (!finalPr) return grid
    const lastWave = nodes.reduce((max, node) => Math.max(max, node.wave), 0)
    return [
      ...grid,
      { id: WORKFLOW_FINAL_PR_NODE_ID, wave: lastWave + 1, lane: 0 },
    ]
  }, [nodes, finalPr])

  const natural = useMemo(
    () =>
      waveGraphSize(cells, {
        nodeWidth: NODE_W,
        nodeHeight: NODE_H,
        waveGap: WAVE_GAP,
        laneGap: LANE_GAP,
      }),
    [cells]
  )
  const { ref, width } = useContainerWidth()
  const boxWidth = natural.width + GRAPH_PAD * 2
  const boxHeight = natural.height + GRAPH_PAD * 2
  // Never up: a graph narrower than its column keeps its true size.
  const scale = width > 0 && boxWidth > width ? width / boxWidth : 1

  if (nodes.length === 0) return null

  return (
    <div
      ref={ref}
      className={cn(`w-full`, className)}
      style={{ height: boxHeight * scale }}
      data-testid="workflow-graph"
    >
      <div
        style={{
          width: boxWidth,
          height: boxHeight,
          padding: GRAPH_PAD,
          transform: scale === 1 ? undefined : `scale(${scale})`,
          transformOrigin: `top left`,
        }}
      >
        <WaveGraph
          nodes={cells}
          edges={edges}
          nodeWidth={NODE_W}
          nodeHeight={NODE_H}
          waveGap={WAVE_GAP}
          laneGap={LANE_GAP}
          idPrefix="workflow-graph"
          nodeProps={(id) => ({
            // The chip inside carries the box; a stack's ghosts and a pick
            // ring paint past the cell's own edge.
            className: `items-stretch`,
            overflowVisible: true,
            testId: `workflow-node-${id}`,
          })}
          renderNode={(id) => {
            if (id === WORKFLOW_FINAL_PR_NODE_ID) {
              return finalPr ? <FinalPrChip {...finalPr} /> : null
            }
            const node = nodeById.get(id)
            if (!node) return null
            return (
              <WorkflowNodeChip
                node={node}
                onSelect={onSelect ? () => onSelect(node.id) : undefined}
              />
            )
          }}
        />
      </div>
    </div>
  )
}

/** The chip's own layout. The BOX is `issue-chip` (styles.css) — the same
 *  6px rect, hairline and wash every other surface names an issue with. */
const CHIP_BODY = `issue-chip flex h-full w-full min-w-0 items-center gap-1 text-xs outline-none transition-[filter,box-shadow] duration-fast`

/** One node: the issue chip, its state glyph leading and its ONE caption
 *  trailing. Nothing under it — the chip says everything. */
export function WorkflowNodeChip({
  node,
  onSelect,
}: {
  node: WorkflowGraphNode
  onSelect?: () => void
}) {
  const tone = TONE_CLASS[node.tone]
  // ONE rule for the slot, ×4: the live dot, else the state's own glyph, else
  // the issue's status — and nothing at all for an issue that has not synced.
  const slot =
    node.glyph ??
    (node.status ? (
      <StatusGlyph
        {...node.status}
        className={cn(`size-3.5`, node.status.className)}
      />
    ) : null)
  const chip = (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={node.selected}
      aria-label={`${node.title} ${node.name}`.trim()}
      title={node.name ? `${node.title} · ${node.name}` : node.title}
      data-testid={`workflow-node-${node.id}-card`}
      data-running={node.running ? `true` : undefined}
      data-proposed={node.proposed ? `true` : undefined}
      data-cycle={node.onCycle ? `true` : undefined}
      // `.issue-chip` is unlayered and beats every border utility, so the two
      // outlines a node can wear are set here rather than with a class.
      style={{
        borderStyle: node.proposed ? `dashed` : undefined,
        borderColor: node.onCycle ? `var(--destructive)` : undefined,
      }}
      className={cn(
        CHIP_BODY,
        onSelect && `cursor-pointer hover:brightness-125`,
        `focus-visible:ring-[3px] focus-visible:ring-ring/50`,
        node.selected && `ring-1 ring-primary`
      )}
    >
      {/* The slot is always THERE, so every chip's identifier starts at the
          same place — an unsynced node simply leaves it empty (desktop
          reserves it the same way). It carries the node's TONE, so a state
          glyph reads in the same colour as its caption; a glyph that paints
          itself — the live dot, the issue's status — keeps its own. */}
      <span
        className={cn(
          `flex size-3.5 shrink-0 items-center justify-center`,
          tone
        )}
        data-testid={`workflow-node-${node.id}-glyph`}
      >
        {slot}
      </span>
      <span
        className={cn(
          `shrink-0 font-mono text-muted-foreground`,
          node.selected && `text-foreground`
        )}
        data-testid={`workflow-node-${node.id}-title`}
      >
        {node.title}
      </span>
      {node.name && (
        <span
          className="min-w-0 flex-1 truncate text-[0.8125rem] font-medium text-foreground"
          data-testid={`workflow-node-${node.id}-name`}
        >
          {node.name}
        </span>
      )}
      <span
        className={cn(`ml-auto shrink-0 pl-1 text-xs`, tone)}
        data-testid={`workflow-node-${node.id}-caption`}
      >
        {node.caption}
      </span>
    </button>
  )
  if (!node.stacked) return chip
  return (
    <IssueChipStack
      className="h-full w-full"
      testId={`workflow-node-${node.id}-stack`}
    >
      {chip}
    </IssueChipStack>
  )
}

/** The ONE final pull request, integration branch → default branch. It is not
 *  a workflow node: nothing selects it. It links out to GitHub once the engine
 *  opened it, and the host may hang its own control off the end (the web's
 *  Merge button). */
function FinalPrChip({ caption, url, trailing }: WorkflowGraphFinalPr) {
  const body = (
    <>
      <MergedIcon className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate text-[0.8125rem] font-medium text-foreground">
        {FINAL_PR_TITLE}
      </span>
    </>
  )
  return (
    <div
      className={CHIP_BODY}
      // Dashed while it is only an intention — inline, because `.issue-chip`
      // is unlayered and beats every border utility.
      style={{ borderStyle: `dashed` }}
      data-testid="workflow-final-pr"
    >
      {url ? (
        <a
          href={url}
          target="_blank"
          rel="noreferrer"
          data-testid="workflow-final-pr-link"
          className="flex min-w-0 flex-1 items-center gap-1 rounded-[4px] outline-none hover:brightness-125 focus-visible:ring-[3px] focus-visible:ring-ring/50"
        >
          {body}
        </a>
      ) : (
        body
      )}
      <span className="ml-auto shrink-0 pl-1 text-xs text-muted-foreground">
        {caption}
      </span>
      {trailing}
    </div>
  )
}
