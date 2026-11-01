// EXP-1495: THE Outline of the primitive sets (React
// @exponential_app/eui-react/primitives `Outline`, Solid
// @exponential_app/eui-solid/primitives, the same markup) = the catalog's
// `Outline` macro on the app's tokens: an anchor list under a 2xs uppercase
// caption, one-line rows `OUTLINE_GEOMETRY.rowHeight` tall, ONE active row =
// the accent background + an `activeBar` px foreground bar on its LEADING
// edge, a muted `· n` trailing count; a press selects the row (scroll-spy
// stays in the host). The plan page's outline column (packages/domain-contract
// fixtures/plan-page.json `layout.outlineRowHeight`/`outlineActiveBar`)
// paints through these; packages/ui holds the numbers equal to the fixture.

export const OUTLINE_GEOMETRY = { rowHeight: 32, activeBar: 2 } as const

export interface OutlineItem {
  id: string
  title: string
  /** A muted `· n` after the title; 0 or unset = none. */
  count?: number | null
}

export const OUTLINE_CLASSES = {
  root: `flex min-w-0 flex-col gap-[2px]`,
  caption: `px-2 py-1 text-[11px] leading-4 font-medium tracking-[0.5px] text-muted-foreground uppercase`,
  /** One row: a button, the active one `data-active=true`. */
  row: `relative flex h-[32px] w-full min-w-0 cursor-pointer items-center gap-1 overflow-hidden rounded-md px-2 text-start text-sm text-muted-foreground outline-none transition-colors duration-fast hover:bg-accent focus-visible:ring-[3px] focus-visible:ring-ring/50 data-[active=true]:bg-accent data-[active=true]:font-medium data-[active=true]:text-foreground`,
  /** The leading-edge bar of the active row. */
  bar: `pointer-events-none absolute inset-y-0 start-0 w-[2px] bg-foreground`,
  title: `min-w-0 flex-1 truncate`,
  count: `shrink-0 text-xs text-muted-foreground tabular-nums`,
} as const

/** The trailing count text (`· n`), or null when there is none. */
export function outlineCountText(count: number | null | undefined): string | null {
  return typeof count === `number` && Number.isFinite(count) && count > 0 ? `· ${count}` : null
}
