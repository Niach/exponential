import { forwardRef, type ComponentProps, type MouseEvent } from "react"
import { cn } from "@/lib/utils"
import { blocksBadgeLabel } from "@/lib/issue-graph"
import {
  railLaneX,
  railNodeX,
  railRowIsEmpty,
  RAIL_NODE_WIDTH,
  type RailLane,
  type RailRow,
} from "@/lib/issue-rail"
import { IssueBlocksPopover } from "@/components/issue-blocks-badge"

// EXP-998: the blocks rail, DRAWN. The RULE is `lib/issue-rail.ts`; this is
// the web's painter — one absolutely positioned layer per list entry at the
// row's right edge (the grid reserves the column, `--issue-rail`), the way
// the tree connector paints per row (`@exp/ui` `TreeGuides`).
//
// AT REST only the nodes show: a small dot flush with the row's right edge —
// a ring in red when something open is in the issue's way, a filled dot when
// the issue only blocks others. That is the hint. Hovering or focusing a dot
// (or hovering the rail) marks the list `data-rail-open` and the arrows fade
// in to the LEFT of the dots: every edge leaves its blocker's node on a
// smooth S-curve out
// to its lane, runs the lane straight, and curves back into the blocked node
// under an arrowhead, the hovered node's own edges in the foreground. The
// list collapses again a beat after the pointer leaves. A CLICK on a dot
// opens the same mini-graph popover as the phone's badge.
//
// The open flag and the edge highlight are DOM attributes toggled in place
// (the rows are memoized; a React state for a hover would re-render a whole
// board). Verticals are `<line>`s (percent-aware, any row height); the
// curves and the arrowhead ride a nested `<svg y="50%">` whose origin IS the
// row's centre and assume the md+ row height (`h-10`) for their reach.

/** Half of a md+ list row (`h-10`): how far a curve reaches from the centre
 *  to the row's edge. A header or a Show-more button only passes lines. */
const ROW_HALF = 20
/** How far a curve stops short of the node's centre: the dot's radius plus
 *  a hairline. */
const NODE_INSET = 6
/** The arrowhead: length along the line and half its height. */
const ARROW_LENGTH = 7
const ARROW_HALF = 4

const INK = `stroke-muted-foreground/45 transition-[stroke,stroke-width] duration-fast data-[hot]:stroke-foreground data-[hot]:[stroke-width:2] data-[cycle]:stroke-destructive`
const HEAD = `fill-muted-foreground/80 transition-[fill] duration-fast data-[hot]:fill-foreground data-[cycle]:fill-destructive`

/** The list root every rail segment is looked up under. It carries the
 *  `group/rail` class and, while the arrows show, `data-rail-open`. */
export const RAIL_ROOT_ATTR = `data-issue-rail-root`
export const RAIL_OPEN_ATTR = `data-rail-open`
/** How long the arrows linger after the pointer leaves the rail. */
const RAIL_CLOSE_DELAY_MS = 180

const closeTimers = new WeakMap<Element, ReturnType<typeof setTimeout>>()

/** Show (or, after a beat, hide) the arrows of the list holding `from`. */
function setRailOpen(from: Element, open: boolean) {
  const root = from.closest(`[${RAIL_ROOT_ATTR}]`)
  if (!root) return
  const pending = closeTimers.get(root)
  if (pending) {
    clearTimeout(pending)
    closeTimers.delete(root)
  }
  if (open) {
    root.setAttribute(RAIL_OPEN_ATTR, ``)
    return
  }
  closeTimers.set(
    root,
    setTimeout(() => {
      root.removeAttribute(RAIL_OPEN_ATTR)
      closeTimers.delete(root)
    }, RAIL_CLOSE_DELAY_MS)
  )
}

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

/** The S-curve between the lane at the row's edge and the node's left side:
 *  a cubic whose tangents are vertical at the lane and horizontal at the
 *  node, so it leaves the lane straight and lands on the node level. `x` is
 *  the lane, `nodeX` the node's centre, `edge` `-ROW_HALF` (from the top)
 *  or `ROW_HALF` (from the bottom). */
export function railCurve(x: number, nodeX: number, edge: number): string {
  const end = nodeX - NODE_INSET
  const bend = edge * 0.55
  return `M ${x} ${edge} C ${x} ${bend}, ${end - (end - x) * 0.45} 0, ${end} 0`
}

/** One lane's slice at one entry: the straight run, the curve(s) into the
 *  node, the arrowhead. */
function LaneSlice({ lane, width }: { lane: RailLane; width: number }) {
  const x = railLaneX(lane.lane, width)
  const nodeX = railNodeX(width)
  const keys = lane.edges.map((edge) => edge.key)
  const edgeAttr = keys.join(` `)
  const title = lane.edges.map((edge) => edge.label).join(`\n`)
  const hover = {
    onMouseEnter: (event: MouseEvent<SVGElement>) => {
      setRailOpen(event.currentTarget, true)
      setHot(event.currentTarget, keys, true)
    },
    onMouseLeave: (event: MouseEvent<SVGElement>) => {
      setHot(event.currentTarget, keys, false)
      setRailOpen(event.currentTarget, false)
    },
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
  if (lane.joinAbove) parts.push(railCurve(x, nodeX, -ROW_HALF))
  else if (lane.top) parts.push(`M ${x} -1000 V ${lane.bottom ? 1000 : 0}`)
  if (lane.joinBelow) parts.push(railCurve(x, nodeX, ROW_HALF))
  else if (lane.bottom && !lane.top) parts.push(`M ${x} 1000 V 0`)
  const d = parts.join(` `)
  if (!d) return null

  // The arrowhead points RIGHT, its tip on the node's left edge.
  const tip = nodeX - NODE_INSET + 1
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
          d={`M ${tip} 0 L ${tip - ARROW_LENGTH} ${-ARROW_HALF} L ${
            tip - ARROW_LENGTH * 0.7
          } 0 L ${tip - ARROW_LENGTH} ${ARROW_HALF} Z`}
          className={cn(`pointer-events-none`, HEAD)}
        />
      )}
    </g>
  )
}

/** The lanes of one entry — the SVG shared by rows and gaps. Hidden until
 *  the list is `data-rail-open`, then fading in with a slide out of the
 *  node column. The svg's own box NEVER takes the pointer: hit testing lives
 *  on each slice's wide hit stroke alone, so what sits under the layer (a
 *  group header's "+" button under the node column) stays clickable while
 *  the arrows linger. */
function RailLanes({ lanes, width }: { lanes: RailLane[]; width: number }) {
  if (lanes.length === 0) return null
  return (
    <svg
      aria-hidden
      focusable="false"
      width={width}
      className="pointer-events-none absolute inset-y-0 left-0 h-full translate-x-1.5 opacity-0 transition-[opacity,transform] duration-fast ease-standard group-data-[rail-open]/rail:translate-x-0 group-data-[rail-open]/rail:opacity-100 motion-reduce:transition-none"
      data-testid="issue-rail-lanes"
    >
      <svg x="0" y="50%" width={width} height="100%" style={{ overflow: `visible` }}>
        {lanes.map((lane) => (
          <LaneSlice key={lane.lane} lane={lane} width={width} />
        ))}
      </svg>
    </svg>
  )
}

/** The node dot, the popover's trigger: forwards Radix's props onto the
 *  button. Hover OR keyboard focus reveals the arrows and lights this node's
 *  own edges (a tabbing user sees the same rail a pointer does); the click
 *  (Radix's) opens the graph. */
const RailNodeButton = forwardRef<
  HTMLButtonElement,
  {
    kind: `blocked` | `blocking`
    label: string
    edgeKeys: string[]
  } & Omit<ComponentProps<`button`>, `children`>
>(function RailNodeButton(
  { kind, label, edgeKeys, onMouseEnter, onMouseLeave, onFocus, onBlur, ...rest },
  ref
) {
  const enter = (el: Element) => {
    setRailOpen(el, true)
    setHot(el, edgeKeys, true)
  }
  const leave = (el: Element) => {
    setHot(el, edgeKeys, false)
    setRailOpen(el, false)
  }
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={label}
      data-testid="issue-rail-node"
      data-kind={kind}
      className="group/node pointer-events-auto absolute inset-y-0 right-0 flex items-center justify-center outline-none"
      style={{ width: RAIL_NODE_WIDTH }}
      // The Radix trigger owns the toggle; this keeps the click off the row
      // underneath (which would open the issue).
      onClick={(event) => event.stopPropagation()}
      onMouseEnter={(event) => {
        enter(event.currentTarget)
        onMouseEnter?.(event)
      }}
      onMouseLeave={(event) => {
        leave(event.currentTarget)
        onMouseLeave?.(event)
      }}
      onFocus={(event) => {
        enter(event.currentTarget)
        onFocus?.(event)
      }}
      onBlur={(event) => {
        leave(event.currentTarget)
        onBlur?.(event)
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
            openOnHover={false}
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
