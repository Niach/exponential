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
 * mounts it in `MAIN_COLUMN_CLASS`, a `md:h-dvh` flex column; the height is
 * `flex-1` here and the column does the arithmetic. EXP-818 removed the agent
 * dock band that used to sit under the card — sessions live in the sidebar
 * and on the Agent page now — so the card is inset 10px on all four sides.
 */
const MAIN_PANEL_BASE = [
  `flex-1 flex flex-col min-h-screen min-w-0`,
  `md:mx-[10px] md:mt-[10px] md:min-h-0`,
  `md:rounded-xl md:border md:border-glass-stroke-card md:bg-glass-panel`,
  `md:overflow-hidden`,
].join(` `)

/**
 * The card with its bottom margin — 10px, symmetric with the other three
 * sides. A separate `mb-` utility rather than an `m-[10px]` shorthand:
 * Tailwind's shorthand/longhand ordering is not a contract worth betting a
 * layout on.
 */
export const MAIN_PANEL_CLASS = `${MAIN_PANEL_BASE} md:mb-[10px]`

/**
 * The team layout's content COLUMN: the card's slot in the `SidebarProvider`
 * row (a horizontal flex container, so `flex-1`/`min-w-0` are about WIDTH). It
 * owns the viewport height from `md` up, which is what lets the card be
 * `flex-1`. On phones there is no card, so the column is a plain pass-through
 * and the window scrolls.
 */
export const MAIN_COLUMN_CLASS = [
  `flex flex-1 flex-col min-w-0`,
  `md:h-dvh`,
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
