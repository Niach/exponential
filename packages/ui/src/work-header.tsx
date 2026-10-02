import type { ReactNode, Ref } from "react"
import { cn } from "./cn"
import { DETAIL_EDGE_TOP_CARD_CLASS } from "./detail-chrome"

// EXP-877: the ONE work header — the issue route and the session route both
// render it (the desktop IDE's `work_header.rs` twin), in the same 896px
// column as the issue body, the transcript and the diff.
//
// EXP-1162: it is a compact BAR now. The large title left it:
//
//   · the Issue face keeps its editable title (and the properties tray) as
//     the first ROWS of its scrolling body, and the bar FLOATS over the
//     scroller's top (`floating`): at rest it carries the right cluster
//     alone, on the title's own line; once the title row scrolled away
//     (`collapsed`) it shows the collapsed title (`CollapsedTitle`) on the
//     edge layer the content passes under.
//   · a face with no title row of its own (Run, diff, Results) is always
//     collapsed: the bar stands above the body, the tray under it, and the
//     edge layer's fade hangs over the body where a hairline used to cut.

/** The bar's height in px — the inset the Issue face's title-collapse
 *  observer reads the scroller's top edge at (`h-12`). */
export const WORK_BAR_HEIGHT = 48

/** The reading column every face shares: issue body, transcript, diff. */
export const WORK_COLUMN_CLASS = `mx-auto w-full max-w-4xl`

export function WorkHeader({
  ref,
  title,
  collapsed = true,
  trailing,
  trailingRef,
  tray,
  floating = false,
  className,
}: {
  ref?: Ref<HTMLDivElement>
  /** The collapsed title (`CollapsedTitle`), shown while `collapsed`. */
  title: ReactNode
  /** The title row scrolled away (always true on a face without one). */
  collapsed?: boolean
  /** The right cluster: face toggle, then the face's own controls. */
  trailing?: ReactNode
  /** The measured cluster — a floating bar's title row keeps clear of it. */
  trailingRef?: Ref<HTMLDivElement>
  /** The issue's properties tray, under the bar. Absent on issue-less runs
   *  and on the Issue face, whose tray scrolls with its body. */
  tray?: ReactNode
  /** The Issue face: the bar takes no height and floats over the scroller it
   *  is the first child of (sticky), so content scrolls under it. */
  floating?: boolean
  className?: string
}) {
  const bar = (
    <div
      ref={ref}
      className={cn(
        `relative z-10`,
        // A floating bar covers the title row's first line at rest: only its
        // cluster may take the pointer.
        floating ? `pointer-events-none` : `shrink-0`,
        tray && `pb-3`,
        className
      )}
      data-testid="work-header"
      data-collapsed={collapsed ? `` : undefined}
    >
      <span
        aria-hidden
        className={cn(
          DETAIL_EDGE_TOP_CARD_CLASS,
          `transition-opacity duration-[160ms] ease-standard`,
          !collapsed && `opacity-0`
        )}
      />
      <div
        className={cn(WORK_COLUMN_CLASS, `flex h-12 items-center gap-2 pl-5`)}
      >
        <div className="min-w-0 flex-1">{collapsed && title}</div>
        {trailing && (
          <div
            ref={trailingRef}
            className="pointer-events-auto flex shrink-0 items-center gap-1 pr-4"
          >
            {trailing}
          </div>
        )}
      </div>
      {tray}
    </div>
  )
  if (!floating) return bar
  return <div className="sticky top-0 z-10 h-0">{bar}</div>
}
