import type { ReactNode } from "react"
import { cn } from "./cn"

// EXP-1162: the detail chrome's shared pieces (contract `detail-chrome.json`,
// ×4): the COLLAPSED TITLE a header breaks into once the title row scrolled
// away, and the two EDGE layers content scrolls under. The phone header
// (`MobileDetailHeader`), the md+ work bar (`WorkBar`) and the floating bottom
// bar (`MobileWorkBar`) all draw these, so the look is one.

/** The top edge: an absolutely placed layer BEHIND its header band, spanning
 *  the band plus the 24px fade under it. The band is its positioned parent
 *  and a stacking context of its own (`relative z-*`). The blur sits on this
 *  layer, never on the band, so the band's menus keep the viewport as their
 *  containing block. */
export const DETAIL_EDGE_TOP_CLASS = `pointer-events-none absolute inset-x-0 top-0 -bottom-6 -z-10 glass-edge-top`

/** The same layer inside the md+ cutout card. */
export const DETAIL_EDGE_TOP_CARD_CLASS = `pointer-events-none absolute inset-x-0 top-0 -bottom-6 -z-10 glass-edge-top-card`

/** The bottom edge behind the phone's floating bar: the bar plus 32px above. */
export const DETAIL_EDGE_BOTTOM_CLASS = `pointer-events-none absolute inset-x-0 bottom-0 -top-8 -z-10 glass-edge-bottom`

/** EXP-1191: the bottom edge over a docked md+ footer (the run composer):
 *  the SCROLLER's class — its last 48px fade out (`feed-fade-bottom`). */
export const DETAIL_FADE_BOTTOM_CLASS = `feed-fade-bottom`

/** The collapse itself: 160ms, rising 4px (the contract's `collapseMs` /
 *  `collapseRise`). */
export const DETAIL_COLLAPSE_ENTER_CLASS = `animate-in fade-in slide-in-from-bottom-1 duration-[160ms] ease-standard motion-reduce:animate-none`

/**
 * What a header shows once the title row scrolled away — and always, on a
 * face with no title row of its own: the mono identifier, small and muted,
 * over the title on ONE truncated line.
 */
export function CollapsedTitle({
  identifier,
  title,
  lead,
  align = `start`,
  animate = true,
  className,
}: {
  /** The mono line above the title; absent on an issue-less run. */
  identifier?: ReactNode
  title: ReactNode
  /** Leads the identifier line: the session-state dot. */
  lead?: ReactNode
  /** `center` under the phone's centred header, `start` in the md+ bar. */
  align?: `start` | `center`
  /** Off for a face that is ALWAYS collapsed: nothing broke, nothing rises. */
  animate?: boolean
  className?: string
}) {
  return (
    <span
      data-testid="collapsed-title"
      className={cn(
        `flex min-w-0 flex-col leading-tight`,
        align === `center` ? `items-center text-center` : `items-start`,
        animate && DETAIL_COLLAPSE_ENTER_CLASS,
        className
      )}
    >
      {identifier !== undefined && identifier !== null && (
        <span className="flex max-w-full items-center font-mono text-[11px] font-normal text-muted-foreground">
          {lead}
          <span className="truncate">{identifier}</span>
        </span>
      )}
      <span className="flex max-w-full items-center text-sm font-medium">
        {(identifier === undefined || identifier === null) && lead}
        <span className="truncate">{title}</span>
      </span>
    </span>
  )
}
