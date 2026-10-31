// EXP-1495: THE Mosaic of the primitive sets (React
// @exponential_app/eui-react/primitives `Mosaic`, Solid
// @exponential_app/eui-solid/primitives, the same markup) = the catalog's
// `Mosaic` macro on the app's tokens: the WIDE picture group. `mosaicTiles`
// = packages/domain-contract plan-page.ts `mosaicTiles` (the SDK imports
// nothing from the app, so the rule is copied; packages/ui holds the two
// equal over plan-page.json `mosaicCases`): 1 = one large tile, 2 = two
// halves side by side, 3+ = the first large on the left (both rows) and the
// next two small stacked on the right, the last carrying `more` = count − 3
// ("+N" over a black 0.55 scrim). Geometry = plan-page.json `mosaic`: `gap`
// between tiles, the large tile `largeFraction` of the width, the group
// `aspect` w/h, `radius` + the glass-stroke hairline, pictures cover. A press
// on a tile selects its index.

export const MOSAIC_GEOMETRY = { gap: 10, largeFraction: 0.6, aspect: 1.6, radius: 10 } as const

export type MosaicTileSize = `large` | `half` | `small`

export interface MosaicTile {
  index: number
  size: MosaicTileSize
  /** The pictures past the third: "+N" over this tile (0 = none). */
  more: number
}

/** The tiles a group of `count` pictures draws. */
export function mosaicTiles(count: number): MosaicTile[] {
  const n = Math.max(0, Math.floor(count))
  if (n === 0) return []
  if (n === 1) return [{ index: 0, size: `large`, more: 0 }]
  if (n === 2) return [0, 1].map((index) => ({ index, size: `half` as const, more: 0 }))
  return [
    { index: 0, size: `large`, more: 0 },
    { index: 1, size: `small`, more: 0 },
    { index: 2, size: `small`, more: n - 3 },
  ]
}

export const MOSAIC_CLASSES = {
  /** The group: `aspect` w/h, a row of the lead tile(s) and the side column. */
  root: `flex aspect-[1.6] w-full min-w-0 flex-row gap-[10px]`,
  /** The right column of the 3+ layout: the two small tiles stacked. */
  side: `flex min-w-0 flex-1 flex-col gap-[10px]`,
  /** One tile: a button holding its picture; `data-size` = the tile size. */
  tile: `relative min-h-0 min-w-0 cursor-pointer overflow-hidden rounded-[10px] border border-glass-stroke-card bg-muted p-0 outline-none transition-opacity duration-fast hover:opacity-90 focus-visible:ring-[3px] focus-visible:ring-ring/50`,
  /** A tile that shares its row or column equally (1, 2, the small two). */
  fill: `flex-1`,
  /** The large tile beside the side column: `largeFraction` of the width. */
  lead: `shrink-0 grow-0 basis-[60%]`,
  image: `absolute inset-0 size-full object-cover`,
  /** The "+N" scrim over the last tile. */
  more: `pointer-events-none absolute inset-0 flex items-center justify-center bg-black/55 text-xl font-semibold text-white`,
} as const
