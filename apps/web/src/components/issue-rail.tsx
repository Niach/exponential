import { forwardRef, type ComponentProps, type MouseEvent } from "react"
import { cn } from "@/lib/utils"
import { blocksBadgeLabel } from "@/lib/issue-graph"
import {
  railLaneX,
  railRowIsEmpty,
  RAIL_NODE_X,
  type RailLane,
  type RailRow,
} from "@/lib/issue-rail"
import { IssueBlocksPopover } from "@/components/issue-blocks-badge"

// EXP-998: the blocks rail, DRAWN. The RULE is `lib/issue-rail.ts`; this is
// the web's painter — one absolutely positioned layer per list entry at the
// row's right edge (the grid reserves the column, `--issue-rail`), the way
// the tree connector paints per row (`@exp/ui` `TreeGuides`).
//
// The look is a git graph's: every edge LEAVES its blocker's node on a smooth
// S-curve out to its lane, runs the lane straight, and curves back INTO the
// blocked node under an arrowhead. A curve spans the half row between the
// node's centre and the row edge, so a node with three blockers fans three
// curves out of it. Nodes: a ring in red when something open is in the
// issue's way, a filled dot when the issue only blocks others.
//
// Hovering the node opens the same mini-graph popover as the phone's badge;
// hovering an arrow names it (`EXP-1 blocks EXP-2`) and lights the whole
// edge across every row it crosses — the segments share an edge key and the
// list root is what they are looked up under.
//
// Verticals are `<line>`s (percent-aware, any row height); the curves and the
// arrowhead ride a nested `<svg y="50%">` whose origin IS the row's centre
// and assume the md+ row height (`h-10`) for their half-row reach.

/** Half of a md+ list row (`h-10`): how far a curve reaches from the centre
 *  to the row's edge. A header or a Show-more button only passes lines. */
const ROW_HALF = 20
/** Where a curve meets the node: the dot's radius plus a hairline. */
const NODE_EDGE = RAIL_NODE_X + 6
/** The arrowhead: length along the line and half its height. */
const ARROW_LENGTH = 7
const ARROW_HALF = 4

const INK = `stroke-muted-foreground/45 transition-[stroke,stroke-width] duration-fast data-[hot]:stroke-foreground data-[hot]:[stroke-width:2] data-[cycle]:stroke-destructive`
const HEAD = `fill-muted-foreground/80 transition-[fill] duration-fast data-[hot]:fill-foreground data-[cycle]:fill-destructive`

/** The list root every rail segment is looked up under. */
export const RAIL_ROOT_ATTR = `data-issue-rail-root`

/** Light up (or drop) every segment of `keys` under the list holding `from`. */
function setHot(from: Element, keys: readonly string[], hot: boolean) {
  const root = from.closest(`[${RAIL_ROOT_ATTR}]`)
  if (!root) return
  for (const key of keys) {
    // An edge key is two row ids — no quote can be in one, but be safe
    // (jsdom has no `CSS.escape`).
    for (const node of root.querySelectorAll(
      `[data-rail-edge~="${key.replace(/["\\]/g, `\\$&`)}"]`
    )) {
      if (hot) node.setAttribute(`data-hot`, ``)
      else node.removeAttribute(`data-hot`)
    }
  }
}

/** The S-curve between the lane at the row's edge and the node's side: a
 *  cubic whose tangents are vertical at the lane and horizontal at the node,
 *  so it leaves the lane straight and lands on the node level. `edge` is
 *  `-ROW_HALF` (from the top) or `ROW_HALF` (from the bottom). */
export function railCurve(x: number, edge: number): string {
  const bend = edge * 0.55
  return `M ${x} ${edge} C ${x} ${bend}, ${NODE_EDGE + (x - NODE_EDGE) * 0.45} 0, ${NODE_EDGE} 0`
}

/** One lane's slice at one entry: the straight run, the curve(s) into the
 *  node, the arrowhead. */
function LaneSlice({ lane }: { lane: RailLane }) {
  const x = railLaneX(lane.lane)
  const keys = lane.edges.map((edge) => edge.key)
  const edgeAttr = keys.join(` `)
  const title = lane.edges.map((edge) => edge.label).join(`\n`)
  const hover = {
    onMouseEnter: (event: MouseEvent<SVGElement>) =>
      setHot(event.currentTarget, keys, true),
    onMouseLeave: (event: MouseEvent<SVGElement>) =>
      setHot(event.currentTarget, keys, false),
  }
  const common = {
    "data-rail-edge": edgeAttr,
    "data-cycle": lane.cycle ? `` : undefined,
    "data-testid": `issue-rail-lane`,
    "data-lane": lane.lane,
  }

  // Which parts to draw. Lanes are shared only where two edges TOUCH at a
  // node, so at any entry a lane holds either one edge passing straight
  // through, or edges that end here — each bending into the node from the
  // side its other end is on.
  const parts: string[] = []
  if (lane.joinAbove) parts.push(railCurve(x, -ROW_HALF))
  else if (lane.top) parts.push(`M ${x} -1000 V ${lane.bottom ? 1000 : 0}`)
  if (lane.joinBelow) parts.push(railCurve(x, ROW_HALF))
  else if (lane.bottom && !lane.top) parts.push(`M ${x} 1000 V 0`)
  const d = parts.join(` `)
  if (!d) return null

  return (
    <g {...hover}>
      <title>{title}</title>
      {/* The hit area first: a wide invisible stroke under the ink. */}
      <path
        d={d}
        fill="none"
        stroke="transparent"
        strokeWidth={10}
        className="pointer-events-auto"
        style={{ pointerEvents: `stroke` }}
      />
      <path
        {...common}
        d={d}
        fill="none"
        strokeWidth={1.5}
        strokeLinecap="round"
        className={cn(`pointer-events-none`, INK)}
      />
      {lane.into && (
        <path
          {...common}
          data-testid="issue-rail-arrow"
          d={`M ${NODE_EDGE - 1} 0 L ${NODE_EDGE + ARROW_LENGTH} ${-ARROW_HALF} L ${
            NODE_EDGE + ARROW_LENGTH * 0.7
          } 0 L ${NODE_EDGE + ARROW_LENGTH} ${ARROW_HALF} Z`}
          className={cn(`pointer-events-none`, HEAD)}
        />
      )}
    </g>
  )
}

/** The lanes of one entry — the SVG shared by rows and gaps. */
function RailLanes({ lanes, width }: { lanes: RailLane[]; width: number }) {
  if (lanes.length === 0) return null
  return (
    <svg
      aria-hidden
      focusable="false"
      width={width}
      className="pointer-events-none absolute inset-y-0 left-0 h-full"
      data-testid="issue-rail-lanes"
    >
      <svg x="0" y="50%" width={width} height="100%" style={{ overflow: `visible` }}>
        {lanes.map((lane) => (
          <LaneSlice key={lane.lane} lane={lane} />
        ))}
      </svg>
    </svg>
  )
}

/** The node dot, the popover's trigger: forwards Radix's props (the
 *  open-on-hover handler among them) onto the button. */
const RailNodeButton = forwardRef<
  HTMLButtonElement,
  {
    kind: `blocked` | `blocking`
    label: string
    edgeKeys: string[]
  } & Omit<ComponentProps<`button`>, `children`>
>(function RailNodeButton(
  { kind, label, edgeKeys, onMouseEnter, onMouseLeave, ...rest },
  ref
) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={label}
      data-testid="issue-rail-node"
      data-kind={kind}
      className="group/node pointer-events-auto absolute inset-y-0 flex w-6 items-center justify-center outline-none"
      style={{ left: RAIL_NODE_X - 12 }}
      // The Radix trigger owns the toggle; this keeps the click off the row
      // underneath (which would open the issue).
      onClick={(event) => event.stopPropagation()}
      onMouseEnter={(event) => {
        setHot(event.currentTarget, edgeKeys, true)
        onMouseEnter?.(event)
      }}
      onMouseLeave={(event) => {
        setHot(event.currentTarget, edgeKeys, false)
        onMouseLeave?.(event)
      }}
      {...rest}
    >
      {/* The dot sits on the row's fill, so a line that lands on it stops
          cleanly at its edge; hover and focus grow a soft halo. */}
      <span
        className={cn(
          `block size-2.5 rounded-full border-2 transition-[box-shadow,transform,border-color,background-color] duration-fast group-hover/node:scale-110 group-hover/node:shadow-[0_0_0_3px] group-focus-visible/node:shadow-[0_0_0_3px]`,
          kind === `blocked`
            ? `border-destructive bg-background shadow-destructive/25`
            : `border-muted-foreground bg-muted-foreground shadow-foreground/15 group-hover/node:border-foreground group-hover/node:bg-foreground`
        )}
      />
    </button>
  )
})

/**
 * A ROW's rail layer: lanes + node. The caller positions it (`className`)
 * over the reserved column; `width` = the rail's width.
 */
export function IssueRailLayer({
  row,
  width,
  issueId,
  teamId,
  className,
}: {
  row: RailRow | null | undefined
  width: number
  issueId: string
  /** The popover's graph scope; absent = an inert node. */
  teamId: string | undefined
  className?: string
}) {
  if (railRowIsEmpty(row) || width === 0) return null
  const { node, counts, lanes } = row!
  const label = blocksBadgeLabel(counts)
  const edgeKeys = [
    ...new Set(
      lanes
        .filter((lane) => lane.into || lane.out)
        .flatMap((lane) => lane.edges.map((edge) => edge.key))
    ),
  ]
  return (
    <div
      className={cn(`pointer-events-none absolute inset-y-0`, className)}
      style={{ width }}
      data-testid="issue-rail"
    >
      <RailLanes lanes={lanes} width={width} />
      {node &&
        (teamId ? (
          <IssueBlocksPopover
            issueId={issueId}
            teamId={teamId}
            label={label}
            trigger={
              <RailNodeButton kind={node} label={label} edgeKeys={edgeKeys} />
            }
          />
        ) : (
          <RailNodeButton kind={node} label={label} edgeKeys={edgeKeys} />
        ))}
    </div>
  )
}

/** A GAP's rail layer (a group header, a "Show more" button): lanes only. */
export function IssueRailGap({
  row,
  width,
  className,
}: {
  row: RailRow | null | undefined
  width: number
  className?: string
}) {
  if (!row || row.lanes.length === 0 || width === 0) return null
  return (
    <div
      className={cn(`pointer-events-none absolute inset-y-0`, className)}
      style={{ width }}
      data-testid="issue-rail-gap"
    >
      <RailLanes lanes={row.lanes} width={width} />
    </div>
  )
}
