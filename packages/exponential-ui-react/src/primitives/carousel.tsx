import { useLayoutEffect, useRef, type ComponentProps } from "react"
import { CAROUSEL_CLASSES, carouselCounterText } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

export { CAROUSEL_GEOMETRY, carouselCounterText } from "@exponential_app/eui/web/primitives"

/** EXP-1495: the Carousel's `counter` part as a primitive: the `i / n` pill
 *  over the slide's top-start corner (its parent is `relative`), hidden from
 *  AT, never intercepting a press. `page` is 0-based. */
export function CarouselCounter({ page, total, className, ...props }: Omit<ComponentProps<`span`>, `children`> & { page: number; total: number }) {
  return (
    <span data-slot="carousel-counter" aria-hidden="true" className={cn(CAROUSEL_CLASSES.counter, className)} {...props}>
      {carouselCounterText(page, total)}
    </span>
  )
}

export type CarouselThumbnailsProps = Omit<ComponentProps<`div`>, `children` | `onChange`> & {
  /** The page count: one thumbnail per page. */
  count: number
  /** The pictures by page; past `count` ignored, a missing one = the empty square. */
  urls: readonly (string | null | undefined)[]
  /** The current page (0-based). */
  page: number
  onPageChange?: (page: number) => void
  /** Each thumbnail's accessible name (default `Page i of n`). */
  pageLabel?: (page: number, total: number) => string
}

const defaultPageLabel = (page: number, total: number) => `Page ${page + 1} of ${total}`

/** EXP-1495: the Carousel's `thumbnails` strip as a primitive: one button per
 *  page, the current one ringed (`data-current`, `aria-current`) and
 *  scrolled into view inside the strip. */
export function CarouselThumbnails({ count, urls, page, onPageChange, pageLabel = defaultPageLabel, className, ...props }: CarouselThumbnailsProps) {
  const strip = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const el = strip.current
    const thumb = el?.children[page] as HTMLElement | undefined
    if (!el || !thumb) return
    const a = el.getBoundingClientRect()
    const b = thumb.getBoundingClientRect()
    const delta = b.left < a.left ? b.left - a.left : b.right > a.right ? b.right - a.right : 0
    if (delta !== 0) el.scrollLeft += delta
  }, [page, count])
  const n = Math.max(0, Math.floor(count))
  return (
    <div ref={strip} data-slot="carousel-thumbnails" className={cn(CAROUSEL_CLASSES.thumbnails, className)} {...props}>
      {Array.from({ length: n }, (_, i) => {
        const current = i === page
        const src = urls[i]
        return (
          <button
            key={i}
            type="button"
            data-slot="carousel-thumbnail"
            data-current={current ? `true` : undefined}
            aria-current={current ? `true` : undefined}
            aria-label={pageLabel(i, n)}
            className={CAROUSEL_CLASSES.thumbnail}
            onClick={() => onPageChange?.(i)}
          >
            {src ? <img src={src} alt="" aria-hidden="true" loading="lazy" decoding="async" draggable={false} className={CAROUSEL_CLASSES.thumbnailImage} /> : null}
          </button>
        )
      })}
    </div>
  )
}
