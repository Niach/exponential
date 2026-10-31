// EXP-1495: the Carousel's `counter` and `thumbnails` on the app's tokens
// (the catalog's Carousel parts `counter`, `thumbnails`, `thumbnail`): the
// "i / n" pill over the slide's top-end corner and the filmstrip of
// `CAROUSEL_GEOMETRY.thumbnail` px squares under it, the current one ringed
// `ring` px in the foreground. The plan page's phone picture group (plan-page
// .json: THE eUI Carousel) paints through these.

export const CAROUSEL_GEOMETRY = { thumbnail: 52, ring: 2, thumbnailGap: 8, counterInset: 8 } as const

/** The counter's text: `i / n` (1-based). */
export function carouselCounterText(page: number, total: number): string {
  return `${page + 1} / ${total}`
}

export const CAROUSEL_CLASSES = {
  /** The pill over the slide; its parent is `relative`. */
  counter: `pointer-events-none absolute end-2 top-2 z-10 rounded-full bg-black/55 px-2 text-xs leading-5 font-medium text-white tabular-nums`,
  /** The filmstrip: one row, scrolling sideways when it overflows. */
  thumbnails: `flex flex-row gap-[8px] overflow-x-auto pt-2 [scrollbar-width:none]`,
  /** One thumbnail: a button; the current one `data-current=true`. */
  thumbnail: `relative size-[52px] shrink-0 cursor-pointer overflow-hidden rounded-md border-2 border-transparent bg-muted p-0 opacity-70 outline-none transition-opacity duration-fast hover:opacity-100 focus-visible:ring-[3px] focus-visible:ring-ring/50 data-[current=true]:border-foreground data-[current=true]:opacity-100`,
  thumbnailImage: `size-full object-cover`,
} as const
