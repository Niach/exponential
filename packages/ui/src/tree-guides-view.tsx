import { cn } from "./cn"
import {
  TREE_BASE,
  TREE_INDENT,
  TREE_RADIUS,
  treeGuideCentre,
  treeGuideIsEmpty,
  type TreeGuide,
} from "./tree-guides"

// EXP-965: the tree connector, DRAWN. The RULE is the pure `treeGuides`
// helper next door (`tree-guides.ts`, mirrored ×4 — the file split is the
// `session-results{,-view}` one: a bare `./tree-guides` must resolve to the
// rule under every bundler in the repo, and bun and tsc disagree about
// `.ts` vs `.tsx`); this is the web's painter:
// one absolutely positioned inline SVG per row, so every nesting site (session
// lists, the run tree, the PR stack, the review queue) connects identically
// and a row only has to be `position: relative`.
//
// Percentages are what keep the elbow on the row's vertical CENTRE whatever
// the row's height: the verticals are `<line>`s (percent-aware), and the
// elbow rides a nested `<svg y="50%">` whose own origin IS that centre.
//
// GAP BRIDGING (the ×4 rule): a list that SPACES its rows would otherwise
// break every line at the gap, because a row can only paint inside itself.
// So a vertical that starts at the row's TOP edge starts one row-gap ABOVE
// it instead, while the tee and the pass-throughs still end at the bottom
// edge — the next row's extension is what covers the space between them.
// That extension is a SECOND layer rather than an `overflow: visible` on the
// first: the elbow's arc is placed by a `50%` that resolves against this
// viewport, so growing the viewport upwards would walk the corner off the
// row's centre by half the gap. Two layers keep the elbow exact at any row
// height and any gap.

/** How wide the guide layer is for a guide — out to the right edge of the
 *  deepest gutter it draws in. */
function guideWidth(guide: TreeGuide, base: number): number {
  const deepest = Math.max(
    guide.elbowAt ?? -1,
    ...(guide.passThrough.length > 0 ? guide.passThrough : [-1])
  )
  return base + TREE_INDENT * (deepest + 1)
}

export function TreeGuides({
  guide,
  base = TREE_BASE,
  gap = 0,
  className,
}: {
  /** This row's geometry (`treeGuides(depths)[index]`). */
  guide: TreeGuide | null | undefined
  /** The row's own left padding at depth 0 — the gutters start there. */
  base?: number
  /** The list's row spacing in px (its `gap-*`), 0 for a gapless list. Every
   *  line that starts at the row's top edge is extended this far above it, so
   *  the connector crosses the space between two rows. */
  gap?: number
  className?: string
}) {
  if (treeGuideIsEmpty(guide)) return null
  const { elbowAt, tee, passThrough } = guide!
  const width = guideWidth(guide!, base)
  const stroke = `var(--glass-stroke-strong)`
  const elbowX = elbowAt === null ? 0 : treeGuideCentre(elbowAt, base)
  // The stub ends at the gutter's right edge — just before the child glyph.
  const stubEnd = elbowAt === null ? 0 : base + TREE_INDENT * (elbowAt + 1)
  // Every line that starts at the top edge: the elbow's own vertical and each
  // ancestor's pass-through. The tee's lower half starts at the centre, so it
  // bridges nothing.
  const bridged =
    elbowAt === null ? passThrough : [...passThrough, elbowAt]
  return (
    <>
    {gap > 0 && bridged.length > 0 && (
      <svg
        aria-hidden
        focusable="false"
        width={width}
        height={gap}
        style={{ top: -gap }}
        className="pointer-events-none absolute left-0"
        data-testid="tree-guides-bridge"
      >
        {bridged.map((level) => (
          <line
            key={`bridge-${level}`}
            x1={treeGuideCentre(level, base)}
            y1="0"
            x2={treeGuideCentre(level, base)}
            y2="100%"
            stroke={stroke}
            strokeWidth={1}
          />
        ))}
      </svg>
    )}
    <svg
      aria-hidden
      focusable="false"
      width={width}
      className={cn(
        `pointer-events-none absolute inset-y-0 left-0 h-full`,
        className
      )}
      data-testid="tree-guides"
    >
      {passThrough.map((level) => (
        <line
          key={`pass-${level}`}
          x1={treeGuideCentre(level, base)}
          y1="0"
          x2={treeGuideCentre(level, base)}
          y2="100%"
          stroke={stroke}
          strokeWidth={1}
        />
      ))}
      {elbowAt !== null && tee && (
        <line
          x1={elbowX}
          y1="50%"
          x2={elbowX}
          y2="100%"
          stroke={stroke}
          strokeWidth={1}
        />
      )}
      {elbowAt !== null && (
        // The nested viewport's origin is the row's vertical CENTRE, so the
        // elbow is plain pixel geometry from there: up (clipped by the outer
        // viewport at the row's top edge), round the corner, out right.
        <svg
          x="0"
          y="50%"
          width={width}
          height="100%"
          style={{ overflow: `visible` }}
        >
          <path
            d={`M ${elbowX} -1000 V ${-TREE_RADIUS} A ${TREE_RADIUS} ${TREE_RADIUS} 0 0 0 ${
              elbowX + TREE_RADIUS
            } 0 H ${stubEnd}`}
            fill="none"
            stroke={stroke}
            strokeWidth={1}
          />
        </svg>
      )}
    </svg>
    </>
  )
}
