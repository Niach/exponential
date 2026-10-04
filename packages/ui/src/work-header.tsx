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
//   · EXP-1191: the tray is ALWAYS in view, on every face — on the issue
//     route it is a sticky row under the floating bar (`WorkStickyTray`),
//     which then owns the edge layer for both (`edge={false}` here).

/** The bar's height in px — the inset the Issue face's title-collapse
 *  observer reads the scroller's top edge at (`h-12`). */
export const WORK_BAR_HEIGHT = 48

/** The reading column every face shares: issue body, transcript, diff. */
export const WORK_COLUMN_CLASS = `mx-auto w-full max-w-4xl`

/** EXP-1191: the column's ONE side inset on md+ (Linear's single edge): the
 *  title, the properties row, description text, tables, cards, rules, the
 *  bar's cluster, the transcript and the composer all start and end on it.
 *  The IDE's `WORK_GUTTER`. */
export const WORK_GUTTER_CLASS = `px-5`

export function WorkHeader({
  ref,
  title,
  collapsed = true,
  trailing,
  trailingRef,
  tray,
  floating = false,
  edge = true,
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
  /** EXP-1191: draw the glass edge layer. Off where a sticky tray under a
   *  floating bar paints one edge for both (`WorkStickyTray`). */
  edge?: boolean
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
      {edge && (
        <span
          aria-hidden
          className={cn(
            DETAIL_EDGE_TOP_CARD_CLASS,
            `transition-opacity duration-[160ms] ease-standard`,
            !collapsed && `opacity-0`
          )}
        />
      )}
      <div
        className={cn(WORK_COLUMN_CLASS, `flex h-12 items-center gap-2 pl-5`)}
      >
        <div className="min-w-0 flex-1">{collapsed && title}</div>
        {trailing && (
          <div
            ref={trailingRef}
            // EXP-1191: the ghost glyphs hang into the gutter so the last
            // one's glyph, not its hit box, ends on the column's edge.
            className="pointer-events-auto flex shrink-0 items-center gap-1 pr-3"
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

/** EXP-1191: the CSS variable a scroller sets to the height of its sticky
 *  chrome (bar + tray), so sticky rows inside it (a diff's file headers)
 *  stick BELOW the chrome instead of under it. */
export const WORK_STICKY_TOP_VAR = `--work-sticky-top`

/**
 * EXP-1191: the issue route's properties tray, kept in view on every face —
 * a sticky row right under the floating bar (`WorkHeader floating
 * edge={false}`). At rest it sits under the title row; once the title
 * scrolled away (`collapsed`) it sticks, and its glass edge layer, reaching
 * up behind the bar, is the one edge the content passes under.
 */
export function WorkStickyTray({
  ref,
  collapsed,
  children,
}: {
  ref?: Ref<HTMLDivElement>
  collapsed: boolean
  children: ReactNode
}) {
  return (
    <div
      ref={ref}
      className="sticky z-[9] pb-3"
      style={{ top: WORK_BAR_HEIGHT }}
      data-testid="work-sticky-tray"
    >
      <span
        aria-hidden
        className={cn(
          DETAIL_EDGE_TOP_CARD_CLASS,
          `-top-12 transition-opacity duration-[160ms] ease-standard`,
          !collapsed && `opacity-0`
        )}
      />
      {children}
    </div>
  )
}
