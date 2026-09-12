import type { ReactNode } from "react"
import { conceptIcon } from "@/lib/icons.generated"
import { Button } from "@/components/ui/button"
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

export function MobileDetailHeader({
  title,
  onBack,
  backLabel = `Back`,
  menu,
  className,
}: {
  /** The identifier or title, centred. */
  title: ReactNode
  /** Absent = no back button (the Agent page's own title bar). */
  onBack?: () => void
  backLabel?: string
  /** The overflow `…` (or any trailing control) — a round ghost button. */
  menu?: ReactNode
  className?: string
}) {
  return (
    <div className={cn(MOBILE_DETAIL_HEADER_CLASS, className)}>
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
}
