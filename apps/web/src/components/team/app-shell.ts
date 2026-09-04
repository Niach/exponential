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
 * fixed descendants, and the fullscreen AgentDock (`fixed inset-0`) would be
 * trapped inside the panel instead of covering the viewport. `app-shell.test.ts`
 * pins both rules.
 */
export const MAIN_PANEL_CLASS = [
  `flex-1 flex flex-col min-h-screen min-w-0`,
  `md:m-[10px] md:min-h-0 md:h-[calc(100dvh-20px)]`,
  `md:rounded-xl md:border md:border-glass-stroke-card md:bg-glass-panel`,
  `md:overflow-hidden`,
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
