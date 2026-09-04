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
