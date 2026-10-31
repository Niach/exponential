import { For, Show, createRenderEffect, splitProps, type ComponentProps } from "solid-js"
import { CAROUSEL_CLASSES, carouselCounterText } from "@exponential_app/eui/web/primitives"

import { cn } from "./cn"

export { CAROUSEL_GEOMETRY, carouselCounterText } from "@exponential_app/eui/web/primitives"

/** EXP-1495: React's `CarouselCounter` (the `i / n` line above the slide; `page` 0-based). */
export function CarouselCounter(props: Omit<ComponentProps<`span`>, `children`> & { page: number; total: number }) {
  const [local, rest] = splitProps(props, [`page`, `total`, `class`])
  return (
    <span data-slot="carousel-counter" aria-hidden="true" class={cn(CAROUSEL_CLASSES.counter, local.class)} {...rest}>
      {carouselCounterText(local.page, local.total)}
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

/** EXP-1495: React's `CarouselThumbnails` (one button per page, the current
 *  one ringed and scrolled into view inside the strip). */
export function CarouselThumbnails(props: CarouselThumbnailsProps) {
  const [local, rest] = splitProps(props, [`count`, `urls`, `page`, `onPageChange`, `pageLabel`, `class`])
  let strip: HTMLDivElement | undefined
  const n = () => Math.max(0, Math.floor(local.count))
  const indexes = () => Array.from({ length: n() }, (_, i) => i)
  const label = (i: number) => (local.pageLabel ?? defaultPageLabel)(i, n())
  createRenderEffect(() => {
    const page = local.page
    n()
    queueMicrotask(() => {
      const el = strip
      const thumb = el?.children[page] as HTMLElement | undefined
      if (!el || !thumb) return
      const a = el.getBoundingClientRect()
      const b = thumb.getBoundingClientRect()
      const delta = b.left < a.left ? b.left - a.left : b.right > a.right ? b.right - a.right : 0
      if (delta !== 0) el.scrollLeft += delta
    })
  })
  return (
    <div ref={strip} data-slot="carousel-thumbnails" class={cn(CAROUSEL_CLASSES.thumbnails, local.class)} {...rest}>
      <For each={indexes()}>
        {(i) => {
          const current = () => i === local.page
          return (
            <button
              type="button"
              data-slot="carousel-thumbnail"
              data-current={current() ? `true` : undefined}
              aria-current={current() ? `true` : undefined}
              aria-label={label(i)}
              class={CAROUSEL_CLASSES.thumbnail}
              onClick={() => local.onPageChange?.(i)}
            >
              <Show when={local.urls[i]}>
                {(src) => <img src={src()} alt="" aria-hidden="true" loading="lazy" decoding="async" draggable={false} class={CAROUSEL_CLASSES.thumbnailImage} />}
              </Show>
            </button>
          )
        }}
      </For>
    </div>
  )
}
