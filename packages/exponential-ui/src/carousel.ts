// EXP-1493: THE Carousel spec (docs/round-6-contract.md §3): what every
// painter shows for a Carousel of `pages` pages. The counter is its own line
// ABOVE the track (never over a page), start-aligned; the dots are one per
// page, the current one `$color.primary`, the others `$color.border`. Pure;
// fixtures/carousel.json locks it and every renderer's `carousel` suite
// replays it against what it PAINTED.

import { resolveRecipe } from "./theme"
import { builtinTheme } from "./themes"
import type { ModeName } from "./theme-types"

export interface CarouselSpecInput {
  theme: string
  mode: ModeName
  pages: number
  page: number
  counter?: boolean
  indicators?: boolean
  thumbnails?: string[]
}

export interface CarouselSpec {
  /** Top to bottom, the root's flow parts (`counter` only with the prop and 2+ pages). */
  order: string[]
  /** The root's gap (px) between consecutive `order` parts. */
  gap: number
  /** null = no counter painted; else a line hugging the root's start edge, `gap` above the track. */
  counter: { text: string; color: string; fontSize: number; height: number } | null
  /** One per page (empty with `indicators: false` or < 2 pages). */
  dots: { color: string; width: number; height: number }[]
}

const num = (v: unknown) => (typeof v === `number` ? v : 0)

export function carouselSpec(input: CarouselSpecInput): CarouselSpec {
  const theme = builtinTheme(input.theme)
  const recipe = (part: string, states: string[] = []) =>
    resolveRecipe(theme, { component: `Carousel`, part, props: { loop: false, indicators: input.indicators ?? true, counter: input.counter ?? false }, states }, input.mode) as Record<string, unknown>
  const many = input.pages >= 2
  const page = many ? ((input.page % input.pages) + input.pages) % input.pages : 0
  const showCounter = many && input.counter === true
  const order = [...(showCounter ? [`counter`] : []), `page`, ...(many && input.thumbnails ? [`thumbnails`] : []), ...(many ? [`controls`] : [])]
  const c = recipe(`counter`)
  return {
    order,
    gap: num(recipe(`root`).gap),
    counter: showCounter ? { text: `${page + 1} / ${input.pages}`, color: String(c.color), fontSize: num(c.fontSize), height: num(c.height) } : null,
    dots:
      many && input.indicators !== false
        ? Array.from({ length: input.pages }, (_, i) => {
            const d = recipe(`indicator`, i === page ? [`selected`] : [])
            return { color: String(d.backgroundColor), width: num(d.width), height: num(d.height) }
          })
        : [],
  }
}
