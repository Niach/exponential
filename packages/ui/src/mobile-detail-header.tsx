import type { ReactNode, Ref } from "react"
import { Button } from "./button"
import { cn } from "./cn"
import { DETAIL_EDGE_TOP_CLASS } from "./detail-chrome"
import { conceptIcon } from "./icons.generated"

// NEW-15: moved from apps/web (components/team/mobile-detail-header.tsx, which
// re-exports it) so the marketing site's phone demos draw the same header.
// EXP-851: the ONE phone header every detail screen wears — the native
// layout: a round ghost back button, the identifier (or title) beside it, the
// page's round actions on the right (header-actions.json). No breadcrumbs anywhere: the board glyph crumb on the issue
// detail, the old "Reviews / MET-3" crumb and the ad-hoc
// back rows on the session pages all became this.

const UiBackIcon = conceptIcon(`ui-back`)

/** The round ghost back control: no circle stroke, no fill (EXP-850 §10). */
export const HEADER_BUTTON_CLASS = `size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground`

/** The bar itself: one line, no card. */
export const MOBILE_DETAIL_HEADER_CLASS = `flex h-12 shrink-0 items-center gap-1 pr-4 pl-2`

/** EXP-1150: the header BAND — the title row plus whatever rides under it
 *  (the Work screen's face tabs), one block. EXP-1162: no hairline closes it
 *  any more — the band sits on the edge layer (`DETAIL_EDGE_TOP_CLASS`),
 *  whose last 24px fade the content out under it. */
const MOBILE_DETAIL_HEADER_BAND_CLASS = `z-20 shrink-0`

/** EXP-1162: the root of a phone detail screen whose header is an OVERLAY —
 *  a fixed-height screen (the dynamic viewport, never the window scrolling:
 *  the header would leave with the page) that positions the band. Its
 *  scroller pads its top by the band's measured height
 *  (`useMeasuredHeight`), so content starts below the band and scrolls
 *  under it. */
export const MOBILE_DETAIL_SCREEN_CLASS = `relative flex h-dvh min-h-0 flex-col`

export function MobileDetailHeader({
  title,
  onBack,
  backLabel = `Back`,
  menu,
  below,
  overlay = false,
  ref,
  className,
}: {
  /** The identifier or title, leading. */
  title: ReactNode
  /** Absent = no back button (the Agent page's own title bar). */
  onBack?: () => void
  backLabel?: string
  /** The trailing actions (`HeaderAction`, in header-actions.json order). */
  menu?: ReactNode
  /** EXP-1150: a row INSIDE the header band, under the title row — the Work
   *  screen's face tabs. The hairline moves under it, so the title and the
   *  tabs read as one header. */
  below?: ReactNode
  /** EXP-1162: the band floats OVER the screen's scroller (the root is
   *  `MOBILE_DETAIL_SCREEN_CLASS`) instead of sitting above it, so content
   *  scrolls under it. Default: in flow, with only the fade hanging below. */
  overlay?: boolean
  /** The band — an overlay's scroller pads by its measured height. */
  ref?: Ref<HTMLDivElement>
  className?: string
}) {
  const row = (
    <div
      className={cn(MOBILE_DETAIL_HEADER_CLASS, className)}
    >
      {onBack ? (
        <Button
          variant="ghost"
          size="icon"
          className={HEADER_BUTTON_CLASS}
          aria-label={backLabel}
          title={backLabel}
          onClick={onBack}
        >
          <UiBackIcon className="size-4" />
        </Button>
      ) : (
        <span aria-hidden className="w-1 shrink-0" />
      )}
      <span className="min-w-0 flex-1 truncate text-left text-sm font-medium">
        {title}
      </span>
      {menu}
    </div>
  )
  return (
    <div
      ref={ref}
      className={cn(
        MOBILE_DETAIL_HEADER_BAND_CLASS,
        overlay ? `absolute inset-x-0 top-0` : `relative`
      )}
      data-testid="mobile-detail-header-band"
    >
      <span aria-hidden className={DETAIL_EDGE_TOP_CLASS} />
      {row}
      {below}
    </div>
  )
}
