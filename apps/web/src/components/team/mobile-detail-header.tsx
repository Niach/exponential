import type { ReactNode } from "react"
import { conceptIcon, Button } from "@exp/ui"
import { cn } from "@/lib/utils"

// EXP-851: the ONE phone header every detail screen wears — the native
// layout, byte for byte: a round ghost back button on the left, the
// identifier (or title) centred, an optional round ghost overflow button on
// the right. No breadcrumbs anywhere: the board glyph crumb on the issue
// detail, the "Reviews / MET-3" crumb on the review detail and the ad-hoc
// back rows on the session pages all became this.
//
// The trailing slot keeps the back button's width whether or not it holds a
// menu, so the title stays optically centred (`HEADER_SLOT_CLASS`).

const UiBackIcon = conceptIcon(`ui-back`)

/** The round ghost control both ends of the header use — no circle stroke, no
 *  fill, the pin toggle's weight (EXP-850 §10). */
export const HEADER_BUTTON_CLASS = `size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground`

/** The fixed slot each end occupies — the balance that centres the title. */
export const HEADER_SLOT_CLASS = `size-9 shrink-0`

/** The bar itself: one line, a hairline under it, no card. */
export const MOBILE_DETAIL_HEADER_CLASS = `flex h-12 shrink-0 items-center gap-1 border-b border-border px-2`

/** EXP-1150: the header BAND — the title row plus whatever rides under it
 *  (the Work screen's face tabs), one block over one hairline. */
const MOBILE_DETAIL_HEADER_BAND_CLASS = `shrink-0 border-b border-border`

export function MobileDetailHeader({
  title,
  onBack,
  backLabel = `Back`,
  menu,
  below,
  className,
}: {
  /** The identifier or title, centred. */
  title: ReactNode
  /** Absent = no back button (the Agent page's own title bar). */
  onBack?: () => void
  backLabel?: string
  /** The overflow `…` (or any trailing control) — a round ghost button. */
  menu?: ReactNode
  /** EXP-1150: a row INSIDE the header band, under the title row — the Work
   *  screen's face tabs. The hairline moves under it, so the title and the
   *  tabs read as one header. */
  below?: ReactNode
  className?: string
}) {
  const row = (
    <div
      className={cn(
        MOBILE_DETAIL_HEADER_CLASS,
        below !== undefined && below !== null && `border-b-0`,
        className
      )}
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
        <span aria-hidden className={HEADER_SLOT_CLASS} />
      )}
      <span className="min-w-0 flex-1 truncate text-center text-sm font-medium">
        {title}
      </span>
      {menu ?? <span aria-hidden className={HEADER_SLOT_CLASS} />}
    </div>
  )
  if (below === undefined || below === null) return row
  return (
    <div className={MOBILE_DETAIL_HEADER_BAND_CLASS} data-testid="mobile-detail-header-band">
      {row}
      {below}
    </div>
  )
}
