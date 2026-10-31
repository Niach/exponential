import { For, Show, splitProps, type ComponentProps } from "solid-js"
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

/** EXP-1495: React's `Mosaic` (the WIDE picture group on `mosaicTiles`). */
export function Mosaic(props: MosaicProps) {
  const [local, rest] = splitProps(props, [`pictures`, `label`, `onSelect`, `class`])
  const tiles = () => mosaicTiles(local.pictures.length)
  const Tile = (p: { tile: MosaicTile; lead: boolean }) => {
    const picture = () => local.pictures[p.tile.index]!
    const alt = () => picture().alt ?? ``
    return (
      <button
        type="button"
        data-slot="mosaic-tile"
        data-size={p.tile.size}
        aria-label={p.tile.more > 0 ? `${alt()}, ${p.tile.more} more` : alt() || undefined}
        class={cn(MOSAIC_CLASSES.tile, p.lead ? MOSAIC_CLASSES.lead : MOSAIC_CLASSES.fill)}
        onClick={() => local.onSelect?.(p.tile.index)}
      >
        <img src={picture().src} alt="" aria-hidden="true" loading="lazy" decoding="async" draggable={false} class={MOSAIC_CLASSES.image} />
        <Show when={p.tile.more > 0}>
          <span aria-hidden="true" class={MOSAIC_CLASSES.more}>
            {`+${p.tile.more}`}
          </span>
        </Show>
      </button>
    )
  }
  return (
    <div data-slot="mosaic" role="group" aria-label={local.label || undefined} class={cn(MOSAIC_CLASSES.root, local.class)} {...rest}>
      <Show when={tiles().length >= 3} fallback={<For each={tiles()}>{(t) => <Tile tile={t} lead={false} />}</For>}>
        <Tile tile={tiles()[0]!} lead />
        <div data-slot="mosaic-side" class={MOSAIC_CLASSES.side}>
          <For each={tiles().slice(1)}>{(t) => <Tile tile={t} lead={false} />}</For>
        </div>
      </Show>
    </div>
  )
}
