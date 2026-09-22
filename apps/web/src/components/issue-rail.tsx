import { forwardRef, type ComponentProps, type MouseEvent } from "react"
import { cn } from "@/lib/utils"
import { blocksBadgeLabel } from "@/lib/issue-graph"
import {
  railLaneX,
  railRowIsEmpty,
  RAIL_NODE_X,
  RAIL_RADIUS,
  type RailLane,
  type RailRow,
} from "@/lib/issue-rail"
import { IssueBlocksPopover } from "@/components/issue-blocks-badge"

// EXP-998: the blocks rail, DRAWN. The RULE is `lib/issue-rail.ts`; this is
// the web's painter — one absolutely positioned layer per list entry at the
// row's right edge (the grid reserves the column, `--issue-rail`), the way
// the tree connector paints per row (`@exp/ui` `TreeGuides`).
//
// A row's layer draws its lane slices (verticals, corners into the node, the
// arrowhead on a blocked node) and the NODE: a red ring when something open
// is in the issue's way, a muted dot when the issue only blocks others.
// Hovering the node opens the same mini-graph popover as the phone's badge;
// hovering an arrow names it (`EXP-1 blocks EXP-2`) and lights up the whole
// edge across every row it crosses — the segments share an edge key and the
// list root is what they are looked up under.
//
// Percentages keep the geometry right at any row height: verticals are
// `<line>`s, and the corner + arrowhead ride a nested `<svg y="50%">` whose
// origin IS the row's centre (the `TreeGuides` recipe).

/** Where the arrow meets the node: the dot's radius plus a hairline. */
const NODE_EDGE = RAIL_NODE_X + 4
/** The arrowhead's length and half-height. */
const ARROW = 4.5

const STROKE_CLASS = `stroke-muted-foreground/60 data-[hot]:stroke-foreground data-[cycle]:stroke-destructive`
const FILL_CLASS = `fill-muted-foreground/60 data-[hot]:fill-foreground data-[cycle]:fill-destructive`

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

/** One lane's slice at one entry. */
function LaneSlice({ lane }: { lane: RailLane }) {
  const x = railLaneX(lane.lane)
  const joins = lane.into || lane.out
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

  // The ink: the vertical(s), the corner or tee into the node, the arrowhead.
  // Everything is drawn relative to the row's CENTRE (the nested viewport),
  // with the verticals overshooting far past the row and clipped by the
  // outer viewport at the row's edges.
  let d = ``
  if (lane.top && lane.bottom) {
    d = `M ${x} -1000 V 1000`
    if (joins) d += ` M ${x} 0 H ${NODE_EDGE}`
  } else if (lane.top) {
    d = joins
      ? `M ${x} -1000 V ${-RAIL_RADIUS} Q ${x} 0 ${x - RAIL_RADIUS} 0 H ${NODE_EDGE}`
      : `M ${x} -1000 V 0`
  } else if (lane.bottom) {
    d = joins
      ? `M ${x} 1000 V ${RAIL_RADIUS} Q ${x} 0 ${x - RAIL_RADIUS} 0 H ${NODE_EDGE}`
      : `M ${x} 1000 V 0`
  }
  if (!d) return null

  return (
    <g {...hover}>
      <title>{title}</title>
      {/* The hit area first: a wide invisible stroke under the ink. */}
      <path
        d={d}
        fill="none"
        stroke="transparent"
        strokeWidth={8}
        className="pointer-events-auto"
        style={{ pointerEvents: `stroke` }}
      />
      <path
        {...common}
        d={d}
        fill="none"
        strokeWidth={1.25}
        strokeLinecap="round"
        className={cn(`pointer-events-none`, STROKE_CLASS)}
      />
      {lane.into && (
        <path
          {...common}
          data-testid="issue-rail-arrow"
          d={`M ${NODE_EDGE} 0 L ${NODE_EDGE + ARROW} ${-ARROW} V ${ARROW} Z`}
          className={cn(`pointer-events-none`, FILL_CLASS)}
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
      className="pointer-events-auto absolute inset-y-0 left-2 flex w-4 items-center justify-center"
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
      <span
        className={cn(
          `block size-2 rounded-full`,
          kind === `blocked`
            ? `border-[1.5px] border-destructive`
            : `bg-muted-foreground`
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
