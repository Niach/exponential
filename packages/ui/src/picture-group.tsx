import { useRef, useState, type TouchEvent } from "react"
import { mosaicTiles, PLAN_PAGE_MOSAIC, PLAN_PAGE_TEST_IDS } from "@exp/domain-contract/plan-page"
import { CarouselCounter } from "@exponential_app/eui-react/primitives/carousel"
import { cn } from "./cn"

// #EXP-1493: a group of pictures. WIDE = THE Mosaic (eUI `Mosaic`,
// plan-page.json `mosaicTiles` + `mosaic`): two = halves, three or more = the
// first large on the left, two small stacked on the right, "+N" over the
// last. PHONES = THE Carousel (eUI `Carousel`): one slide at a time with the
// "i / n" counter and dots, a swipe steps; `thumbnails` adds the filmstrip
// (the story gallery). A press on a picture = `onOpen(index)`.

export interface GroupPicture {
  src: string
  alt: string
  width?: number | null
  height?: number | null
}

const SWIPE_PX = 40

export function Mosaic({
  pictures,
  onOpen,
  className,
}: {
  pictures: readonly GroupPicture[]
  onOpen?: (index: number) => void
  className?: string
}) {
  const tiles = mosaicTiles(pictures.length)
  const { gap, largeFraction, aspect, radius } = PLAN_PAGE_MOSAIC
  const tile = (at: number) => {
    const entry = tiles[at]
    const picture = entry ? pictures[entry.index] : undefined
    if (!entry || !picture) return null
    return (
      <button
        key={entry.index}
        type="button"
        data-slot="mosaic-tile"
        data-size={entry.size}
        aria-label={picture.alt}
        onClick={() => onOpen?.(entry.index)}
        className="relative min-h-0 min-w-0 overflow-hidden border border-glass-stroke bg-muted outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
        style={{ borderRadius: radius }}
      >
        <img src={picture.src} alt={picture.alt} draggable={false} className="size-full object-cover" />
        {entry.more > 0 && (
          <span className="absolute inset-0 flex items-center justify-center bg-black/55 text-lg font-semibold text-white">
            +{entry.more}
          </span>
        )}
      </button>
    )
  }
  return (
    <div
      data-testid={PLAN_PAGE_TEST_IDS.mosaic}
      className={cn(`grid w-full`, className)}
      style={{
        gap,
        aspectRatio: `${aspect}`,
        gridTemplateColumns:
          tiles.length === 2 ? `1fr 1fr` : tiles.length >= 3 ? `${largeFraction}fr ${1 - largeFraction}fr` : `1fr`,
        gridTemplateRows: tiles.length >= 3 ? `1fr 1fr` : `1fr`,
      }}
    >
      {tiles.length >= 3 ? (
        <>
          <div className="row-span-2 grid min-h-0">{tile(0)}</div>
          {tile(1)}
          {tile(2)}
        </>
      ) : (
        tiles.map((_, at) => tile(at))
      )}
    </div>
  )
}

export function PictureCarousel({
  pictures,
  index: controlled,
  onIndex,
  onOpen,
  thumbnails = false,
  fit = `cover`,
  className,
  slideClassName,
}: {
  pictures: readonly GroupPicture[]
  /** Controlled slide (absent = its own). */
  index?: number
  onIndex?: (index: number) => void
  onOpen?: (index: number) => void
  /** The filmstrip under the slide (the story gallery). */
  thumbnails?: boolean
  fit?: `cover` | `contain`
  className?: string
  slideClassName?: string
}) {
  const [own, setOwn] = useState(0)
  const count = pictures.length
  const at = Math.min(Math.max(0, controlled ?? own), Math.max(0, count - 1))
  const go = (next: number) => {
    const clamped = Math.min(Math.max(0, next), count - 1)
    setOwn(clamped)
    onIndex?.(clamped)
  }
  const from = useRef<{ x: number; y: number } | null>(null)
  const onTouchStart = (event: TouchEvent<HTMLDivElement>) => {
    const touch = event.touches[0]
    from.current = touch ? { x: touch.clientX, y: touch.clientY } : null
  }
  const onTouchEnd = (event: TouchEvent<HTMLDivElement>) => {
    const start = from.current
    from.current = null
    const touch = event.changedTouches[0]
    if (!start || !touch) return
    const dx = touch.clientX - start.x
    if (Math.abs(dx) <= Math.abs(touch.clientY - start.y) || Math.abs(dx) < SWIPE_PX) return
    // A swipe inside the carousel steps the picture, never its host.
    event.stopPropagation()
    go(at + (dx < 0 ? 1 : -1))
  }
  const picture = pictures[at]
  if (!picture) return null
  return (
    <div data-testid={PLAN_PAGE_TEST_IDS.carousel} data-index={at} className={cn(`flex min-w-0 flex-col gap-2`, className)}>
      {/* THE eUI Carousel counter (round-6 §3): its own line above the slide, start-aligned, no pill. */}
      {count > 1 && <CarouselCounter page={at} total={count} />}
      <div
        data-slot="carousel-slide"
        className={cn(`relative overflow-hidden`, slideClassName)}
        style={{ borderRadius: fit === `cover` ? PLAN_PAGE_MOSAIC.radius : undefined }}
        onTouchStart={onTouchStart}
        onTouchEnd={onTouchEnd}
      >
        <button
          type="button"
          aria-label={picture.alt}
          onClick={() => onOpen?.(at)}
          className={cn(
            `block size-full outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
            fit === `cover` && `border border-glass-stroke bg-muted`
          )}
          style={{
            borderRadius: fit === `cover` ? PLAN_PAGE_MOSAIC.radius : undefined,
            aspectRatio:
              fit === `cover`
                ? picture.width && picture.height
                  ? `${picture.width} / ${picture.height}`
                  : `${PLAN_PAGE_MOSAIC.aspect}`
                : undefined,
          }}
        >
          <img
            src={picture.src}
            alt={picture.alt}
            draggable={false}
            className={cn(`size-full`, fit === `cover` ? `object-cover` : `object-contain`)}
            style={{ borderRadius: fit === `cover` ? PLAN_PAGE_MOSAIC.radius : undefined }}
          />
        </button>
      </div>
      {count > 1 &&
        (thumbnails ? (
          <div data-slot="carousel-thumbnails" className="flex gap-1.5 overflow-x-auto">
            {pictures.map((entry, i) => (
              <button
                key={i}
                type="button"
                aria-label={entry.alt}
                aria-current={i === at || undefined}
                onClick={() => go(i)}
                className={cn(
                  `size-13 shrink-0 overflow-hidden rounded-md border outline-none`,
                  i === at ? `border-2 border-foreground` : `border-glass-stroke opacity-80`
                )}
              >
                <img src={entry.src} alt="" draggable={false} className="size-full object-cover" />
              </button>
            ))}
          </div>
        ) : (
          <div data-slot="carousel-dots" className="flex justify-center gap-1.5">
            {pictures.map((_, i) => (
              <button
                key={i}
                type="button"
                aria-label={`${i + 1} / ${count}`}
                onClick={() => go(i)}
                aria-current={i === at || undefined}
                className={cn(`size-2 rounded-full`, i === at ? `bg-primary` : `bg-border`)}
              />
            ))}
          </div>
        ))}
    </div>
  )
}
