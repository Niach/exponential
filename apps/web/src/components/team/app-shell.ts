/**
 * EXP-723: the Linear-style CUTOUT panel.
 *
 * The team layout's content column is a rounded card floating on the page
 * gradient, with the sidebar sitting directly on that ground. Two rules make
 * it work, and both are load-bearing:
 *
 *   1. Every card property is `md:`-prefixed. On phones the column stays
 *      full-bleed — a card there would only shrink the reading width and put a
 *      hairline under the MobileTabBar, which floats over the content anyway.
 *   2. The margins and the height are PX literals, not rem steps: the md+ root
 *      font is 1.15625rem (styles.css L7-15), so `m-2.5` would be 11.6px and
 *      `h-[calc(100dvh-1.25rem)]` would leave the card 3px short of the
 *      viewport. 10px all round, 20px off the viewport height.
 *
 * The card must NEVER gain `transform`, `filter`, `backdrop-filter`,
 * `will-change` or `contain`: each of those makes it a containing block for
 * `position: fixed` descendants, and every full-viewport overlay the panel
 * hosts (dialogs, sheets, the mobile takeovers) would be trapped inside the
 * card instead of covering the viewport. `app-shell.test.ts` pins both rules.
 *
 * EXP-771: the card no longer owns the whole viewport height. The team layout
 * mounts it in `MAIN_COLUMN_CLASS`, a `md:h-dvh` flex column that also carries
 * the agent dock BELOW the card, on the bare page ground — IDE parity, where
 * the title band sits outside the card too. So the height is `flex-1` here and
 * the column does the arithmetic.
 */
const MAIN_PANEL_BASE = [
  `flex-1 flex flex-col min-h-screen min-w-0`,
  `md:mx-[10px] md:mt-[10px] md:min-h-0`,
  `md:rounded-xl md:border md:border-glass-stroke-card md:bg-glass-panel`,
  `md:overflow-hidden`,
].join(` `)

/**
 * The card with its DOCKLESS bottom margin — 10px, symmetric with the other
 * three sides. Kept as the exported constant because the invariants above are
 * about the card itself, not about which margin it ends on;
 * `mainPanelClass(true)` is the docked variant.
 */
export const MAIN_PANEL_CLASS = `${MAIN_PANEL_BASE} md:mb-[10px]`

/**
 * The card's classes for a given dock state. With the dock up the gap between
 * card and band is 6px, not 10px: the band is chrome BELONGING to the card
 * (its tabs are the card's contents), so it rides closer than the window
 * inset. Two separate `mb-` utilities rather than an override on `m-[10px]` —
 * Tailwind's shorthand/longhand ordering is not a contract worth betting a
 * layout on.
 */
export function mainPanelClass(docked: boolean): string {
  return docked ? `${MAIN_PANEL_BASE} md:mb-[6px]` : MAIN_PANEL_CLASS
}

/**
 * The team layout's content COLUMN: the card plus, under it, the agent dock.
 *
 * It takes the card's old slot in the `SidebarProvider` row (a horizontal flex
 * container, so `flex-1`/`min-w-0` are about WIDTH) and owns the viewport
 * height from `md` up, which is what lets the card be `flex-1` and the dock a
 * fixed band flush with the window's bottom edge. On phones there is no dock
 * and no card, so the column is a plain pass-through and the window scrolls.
 */
export const MAIN_COLUMN_CLASS = [
  `flex flex-1 flex-col min-w-0`,
  `md:h-dvh`,
].join(` `)

/**
 * EXP-771: the agent dock's band — OUTSIDE the card, on the page ground.
 *
 * No fill and no border: the ground shows through, so the band reads as part
 * of the window chrome rather than a second card. It shares the card's 10px
 * side margins and insets its chips a further 8px, which puts the first chip
 * 8px inside the card's left edge (the IDE title band's geometry).
 *
 * PX literals, like the card: the md+ root font is 1.15625rem, so `h-9` would
 * be 41.6px and `px-2` 9.25px. No `md:` prefixes are needed — the dock renders
 * nothing at all below `md` (`useIsMobile`, same 768px breakpoint).
 */
export const DOCK_BAND_CLASS = [
  `flex h-[36px] shrink-0 items-center gap-1 overflow-x-auto`,
  `mx-[10px] px-[8px]`,
].join(` `)

/**
 * The wrapper the team layout mounts the `<Outlet />` in — the panel's ONE
 * scrollport from `md` up.
 *
 * The panel itself is `md:overflow-hidden` at a definite height, so on md+ the
 * window never scrolls: a route that relied on window scroll (every
 * `settings/*` page is a plain `max-w-4xl` column) would simply be cut off at
 * the panel's bottom edge. `md:overflow-y-auto` here makes this wrapper the
 * scroller instead.
 *
 * Routes that own a viewport-sized scroller of their own (the issue detail
 * view is `h-full min-h-0` with an inner `overflow-y-auto`) keep working and do
 * NOT double-scroll: the wrapper is a min-sized flex child of a definite-height
 * column, so `h-full` fills it exactly and never overflows it.
 *
 * On phones the column grows with the page and the window scrolls, which is
 * why the scroller is `md:`-gated.
 */
export const MAIN_OUTLET_CLASS = [
  `flex-1 min-h-0 min-w-0 overflow-x-clip`,
  `md:overflow-y-auto`,
].join(` `)

/**
 * EXP-760: the issue detail's sticky band — title + properties — pinned to the
 * top of the view's own scroller, IDE parity (`issue_header.rs`).
 *
 * `glass-chrome-card`, not `glass-chrome-top`: the band sits INSIDE the cutout
 * panel, whose `--glass-fill-panel` layer the window-edge scrim does not
 * account for (see styles.css) — the panel's own ground has to be repainted
 * under the scrim or the band reads as a black bar over the card.
 *
 * The blur is safe HERE and not on `MAIN_PANEL_CLASS`: this node hosts no
 * `position: fixed` overlay of its own (the property pickers portal to
 * `document.body`, outside it), while the panel is the ancestor of every
 * dialog and sheet the app opens.
 */
export const DETAIL_STICKY_BAND_CLASS = [
  `sticky top-0 z-10`,
  `glass-chrome-card border-b border-glass-stroke`,
].join(` `)
