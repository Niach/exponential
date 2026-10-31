import type { ComponentProps } from "react"
import { MOSAIC_CLASSES, mosaicTiles, type MosaicTile } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

export { MOSAIC_GEOMETRY, mosaicTiles } from "@exponential_app/eui/web/primitives"
export type { MosaicTile, MosaicTileSize } from "@exponential_app/eui/web/primitives"

export interface MosaicPicture {
  src: string
  alt?: string
}

export type MosaicProps = Omit<ComponentProps<`div`>, `onSelect` | `children`> & {
  pictures: readonly MosaicPicture[]
  /** The group's accessible name. */
  label?: string
  /** A tile press: its picture index (0, 1 or 2; the host opens its lightbox). */
  onSelect?: (index: number) => void
}

/** EXP-1495: the catalog's `Mosaic` macro as a primitive: the WIDE picture
 *  group (`mosaicTiles`: 1 large, 2 halves, 3+ = the large lead + two small
 *  stacked, the last carrying "+N"). */
export function Mosaic({ pictures, label, onSelect, className, ...props }: MosaicProps) {
  const tiles = mosaicTiles(pictures.length)
  const tile = (t: MosaicTile, lead: boolean) => {
    const picture = pictures[t.index]!
    const alt = picture.alt ?? ``
    return (
      <button
        key={t.index}
        type="button"
        data-slot="mosaic-tile"
        data-size={t.size}
        aria-label={t.more > 0 ? `${alt}, ${t.more} more` : alt || undefined}
        className={cn(MOSAIC_CLASSES.tile, lead ? MOSAIC_CLASSES.lead : MOSAIC_CLASSES.fill)}
        onClick={() => onSelect?.(t.index)}
      >
        <img src={picture.src} alt="" aria-hidden="true" loading="lazy" decoding="async" draggable={false} className={MOSAIC_CLASSES.image} />
        {t.more > 0 ? (
          <span aria-hidden="true" className={MOSAIC_CLASSES.more}>
            {`+${t.more}`}
          </span>
        ) : null}
      </button>
    )
  }
  return (
    <div data-slot="mosaic" role="group" aria-label={label || undefined} className={cn(MOSAIC_CLASSES.root, className)} {...props}>
      {tiles.length >= 3 ? (
        <>
          {tile(tiles[0]!, true)}
          <div data-slot="mosaic-side" className={MOSAIC_CLASSES.side}>
            {tiles.slice(1).map((t) => tile(t, false))}
          </div>
        </>
      ) : (
        tiles.map((t) => tile(t, false))
      )}
    </div>
  )
}
