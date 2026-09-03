import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

// EXP-698 r5 — the activity feed's ONE row shape, ported from iOS
// `TimelineRow` (CommentThreadView.swift) and mirrored by the IDE's
// `timeline_row`: a 28px gutter carrying the row's marker (a comment avatar, an
// event glyph, a 6px dot) with a 1px rail at the card stroke running above and
// below it, and the row's own content beside it.
//
// The rail is DRAWN, not inherited: absolutely positioned segments that BLEED
// past the row's own vertical padding (so one row's rail ends exactly where
// the next one's begins) and stop `RAIL_BREAK` short of the marker on both
// sides — a 12px gap around any marker, whatever its size. The first row of
// the feed passes `lineAbove={false}` and the last `lineBelow={false}`: the
// rail spans the history, it does not point at the composer.
//
// Both segments therefore need ROOM, and a caller has to leave it: the break
// eats `RAIL_BREAK` above the marker and `RAIL_BREAK` below it, so a row whose
// `markerTop + padY` or whose slack under the marker falls short of that draws
// no segment at all and the spine breaks between two rows. The short rows (a
// 14px event glyph, the 6px creation dot) get there with the default `padY`
// and a `min-h-5` content line.
const GUTTER = 28
const RAIL_BREAK = 6

export function TimelineRow({
  marker,
  markerSize,
  markerTop = 2,
  padY = 6,
  lineAbove = true,
  lineBelow = true,
  className,
  children,
}: {
  /** Centred in the gutter: the avatar, the event glyph, the creation dot. */
  marker: ReactNode
  /** The marker's own box, so the rail knows where to break. */
  markerSize: number
  /** Distance from the row's content top to the marker, aligning it with the
   * row's first text line. */
  markerTop?: number
  /** The row's vertical padding, which the rail segments bleed across. */
  padY?: number
  lineAbove?: boolean
  lineBelow?: boolean
  className?: string
  children: ReactNode
}) {
  const aboveHeight = markerTop + padY - RAIL_BREAK
  const belowTop = markerTop + markerSize + RAIL_BREAK
  return (
    <div
      className={cn(`flex gap-2`, className)}
      style={{ paddingTop: padY, paddingBottom: padY }}
    >
      <div className="relative shrink-0" style={{ width: GUTTER }} aria-hidden>
        {lineAbove && aboveHeight > 0 && (
          <span
            data-timeline-rail="above"
            className="absolute left-1/2 w-px -translate-x-1/2 bg-glass-stroke-card"
            style={{ top: -padY, height: aboveHeight }}
          />
        )}
        {lineBelow && (
          <span
            data-timeline-rail="below"
            className="absolute left-1/2 w-px -translate-x-1/2 bg-glass-stroke-card"
            style={{ top: belowTop, bottom: -padY }}
          />
        )}
        <div
          data-timeline-marker
          className="relative flex items-center justify-center"
          style={{ marginTop: markerTop, height: markerSize }}
        >
          {marker}
        </div>
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  )
}
