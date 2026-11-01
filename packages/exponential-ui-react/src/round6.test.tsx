// EXP-1495 (docs/round-6-contract.md): the React halves of round 6 — the
// Carousel's `counter` pill (over the track, `$string.pageCounter`, hidden
// from AT, never hit) and `thumbnails` strip (one button per PAGE before
// the controls, the current one `selected` + aria-current, a press pages
// like a dot), only with 2+ pages; the Outline and Mosaic macros painting
// through Box/Text/Image (aspectRatio, absolute inset, Image fit cover); the
// primitive twins Outline, Mosaic, CarouselCounter, CarouselThumbnails.

import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"
import { CarouselCounter, CarouselThumbnails, Mosaic, Outline } from "./primitives"

afterEach(() => cleanup())

/** The node sheet rule of one node (`.xui-n-<id>{…}`), from the surface's <style>s. */
const rule = (id: string) => {
  const css = [...document.querySelectorAll(`style`)].map((s) => s.textContent ?? ``).join(``)
  const at = css.indexOf(`.xui-n-${id.replace(/\./g, `_2e_`)}{`)
  return at < 0 ? `` : css.slice(at, css.indexOf(`}`, at) + 1)
}
const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root
const pages = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `p${i}`, component: `Text`, props: { text: `Page ${i + 1}` } }))
const carousel = (props: Record<string, unknown>, n = 3) => tree({ id: `cr`, component: `Carousel`, props, on: { change: { event: { name: `paged` } } }, children: pages(n) } as unknown as NestedNode)

describe(`Carousel counter`, () => {
  it(`paints "page / total" over the track, hidden from AT, never hit`, () => {
    const { container } = render(<ExponentialSurface root={carousel({ counter: true, page: 1 })} theme="neutral" id="c1" />)
    const counter = container.querySelector<HTMLElement>(`[data-xui-part="Carousel/counter"]`)!
    expect(counter.textContent).toBe(`2 / 3`)
    expect(counter.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(counter.getAttribute(`data-xui-id`)).toBe(`cr.counter`)
    expect(counter.className).toContain(`xui-Carousel-counter`)
    // Its positioned parent holds the track (the pill stays put while the track scrolls).
    expect(counter.parentElement!.classList.contains(`xui-carousel-stage`)).toBe(true)
    expect(counter.parentElement!.querySelector(`.xui-carousel-track`)).not.toBeNull()
  })

  it(`follows the page`, () => {
    const { container } = render(<ExponentialSurface root={carousel({ counter: true })} theme="neutral" id="c2" />)
    fireEvent.click(container.querySelector(`[data-xui-part="Carousel/next"]`)!)
    expect(container.querySelector(`[data-xui-part="Carousel/counter"]`)!.textContent).toBe(`2 / 3`)
  })

  it(`none with one page, none without the prop`, () => {
    const one = render(<ExponentialSurface root={carousel({ counter: true }, 1)} theme="neutral" id="c3" />)
    expect(one.container.querySelector(`[data-xui-part="Carousel/counter"]`)).toBeNull()
    expect(one.container.querySelector(`.xui-carousel-stage`)).toBeNull()
    cleanup()
    const off = render(<ExponentialSurface root={carousel({})} theme="neutral" id="c4" />)
    expect(off.container.querySelector(`[data-xui-part="Carousel/counter"]`)).toBeNull()
  })
})

describe(`Carousel thumbnails`, () => {
  const urls = [`https://example.com/1.jpg`, ``, `https://example.com/3.jpg`, `https://example.com/4.jpg`]

  it(`one button per page between the track and the controls; extras ignored, a missing one = empty`, () => {
    const { container } = render(<ExponentialSurface root={carousel({ thumbnails: urls })} theme="neutral" id="t1" />)
    const strip = container.querySelector<HTMLElement>(`[data-xui-part="Carousel/thumbnails"]`)!
    expect(strip.getAttribute(`data-xui-id`)).toBe(`cr.thumbnails`)
    const root = container.querySelector(`[data-xui-id="cr"]`)!
    const kids = [...root.children]
    expect(kids.indexOf(strip)).toBe(kids.indexOf(container.querySelector(`.xui-carousel-track`)!) + 1)
    expect(strip.nextElementSibling!.getAttribute(`data-xui-part`)).toBe(`Carousel/controls`)
    const thumbs = strip.querySelectorAll<HTMLButtonElement>(`[data-xui-part="Carousel/thumbnail"]`)
    expect(thumbs).toHaveLength(3)
    expect([...thumbs].map((t) => t.getAttribute(`data-xui-id`))).toEqual([`cr.thumbnail.0`, `cr.thumbnail.1`, `cr.thumbnail.2`])
    expect([...thumbs].map((t) => t.getAttribute(`aria-label`))).toEqual([`Page 1 of 3`, `Page 2 of 3`, `Page 3 of 3`])
    expect([...thumbs].map((t) => t.querySelector(`img`)?.getAttribute(`src`) ?? null)).toEqual([`https://example.com/1.jpg`, null, `https://example.com/3.jpg`])
    expect(thumbs[0]!.querySelector(`img`)!.getAttribute(`alt`)).toBe(``)
    expect(thumbs[0]!.getAttribute(`aria-current`)).toBe(`true`)
    expect(thumbs[0]!.getAttribute(`data-current`)).toBe(`true`)
    expect(thumbs[0]!.getAttribute(`data-xs`)).toBe(`selected`)
    expect(thumbs[1]!.getAttribute(`aria-current`)).toBeNull()
    expect(thumbs[1]!.getAttribute(`data-xs`)).toBeNull()
  })

  it(`a press goes to that page and fires change {page} like a dot`, () => {
    const actions: SurfaceActionEvent[] = []
    const { container } = render(<ExponentialSurface root={carousel({ thumbnails: urls })} theme="neutral" id="t2" host={{ onAction: (e) => void actions.push(e) }} />)
    fireEvent.click(container.querySelectorAll(`[data-xui-part="Carousel/thumbnail"]`)[2]!)
    expect(actions.map((a) => [a.event, a.name, a.payload?.page])).toEqual([[`change`, `paged`, 2]])
    const thumbs = container.querySelectorAll(`[data-xui-part="Carousel/thumbnail"]`)
    expect(thumbs[2]!.getAttribute(`aria-current`)).toBe(`true`)
    expect(thumbs[0]!.getAttribute(`aria-current`)).toBeNull()
    expect(container.querySelector(`.xui-Carousel-page[data-xs="selected"]`)!.textContent).toBe(`Page 3`)
    // The dots follow too.
    expect(container.querySelectorAll(`.xui-Carousel-indicator`)[2]!.getAttribute(`aria-selected`)).toBe(`true`)
  })

  it(`none with one page`, () => {
    const { container } = render(<ExponentialSurface root={carousel({ thumbnails: urls }, 1)} theme="neutral" id="t3" />)
    expect(container.querySelector(`[data-xui-part="Carousel/thumbnails"]`)).toBeNull()
  })

  it(`the kitchen sink's Carousel: counter + two thumbnails`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `carousel`, component: `Carousel`, props: { page: 0, indicators: true, counter: true, thumbnails: [`https://example.com/1.jpg`, `https://example.com/2.jpg`] }, children: [{ id: `page-1`, component: `Image`, props: { src: `https://example.com/1.jpg`, alt: `One` } }, { id: `page-2`, component: `Image`, props: { src: `https://example.com/2.jpg`, alt: `Two` } }] } as unknown as NestedNode)} theme="neutral" id="ks" />)
    expect(container.querySelector(`[data-xui-part="Carousel/counter"]`)!.textContent).toBe(`1 / 2`)
    expect(container.querySelectorAll(`[data-xui-part="Carousel/thumbnail"] img`)).toHaveLength(2)
  })
})

describe(`Outline + Mosaic macros paint through Box/Text/Image`, () => {
  it(`Outline: navigation, the active row with its bar, the count`, () => {
    const { container } = render(<ExponentialSurface root={tree({ id: `o`, component: `Outline`, props: { caption: `Outline`, value: `b`, items: [{ id: `a`, title: `Alpha` }, { id: `b`, title: `Beta`, count: 5 }] } } as unknown as NestedNode)} theme="neutral" id="o1" />)
    const root = container.querySelector(`[data-xui-id="o"]`)!
    expect(root.getAttribute(`role`)).toBe(`navigation`)
    expect(root.getAttribute(`aria-label`)).toBe(`Outline`)
    const bar = container.querySelector<HTMLElement>(`[data-xui-id="o.row.1.bar"]`)!
    expect(bar).not.toBeNull()
    expect(rule(`o.row.1.bar`)).toContain(`position:absolute`)
    expect(rule(`o.row.1.bar`)).toMatch(/inset-inline-start:0(px)?[;}]/)
    expect(container.querySelector(`[data-xui-id="o.row.0.bar"]`)).toBeNull()
    expect(container.querySelector(`[data-xui-id="o.row.1"]`)!.getAttribute(`aria-current`)).toBe(`location`)
    expect(container.querySelector(`[data-xui-id="o.row.1.count"]`)!.textContent).toBe(`· 5`)
  })

  it(`Outline: a row press fires select {id}`, () => {
    const actions: SurfaceActionEvent[] = []
    const root = tree({ id: `o`, component: `Outline`, props: { items: [{ id: `a`, title: `Alpha` }, { id: `b`, title: `Beta` }] }, on: { select: { event: { name: `jump` } } } } as unknown as NestedNode)
    const { container } = render(<ExponentialSurface root={root} theme="neutral" id="o2" host={{ onAction: (e) => void actions.push(e) }} />)
    fireEvent.click(container.querySelector(`[data-xui-id="o.row.1"]`)!)
    expect(actions.map((a) => [a.name, a.context.id ?? a.payload?.id])).toEqual([[`jump`, `b`]])
  })

  it(`Mosaic: the 1.6 group, the 60 % lead, cover pictures laid over each tile, "+N"`, () => {
    const pictures = [1, 2, 3, 4, 5].map((i) => ({ src: `https://example.com/${i}.png`, alt: `P${i}` }))
    const { container } = render(<ExponentialSurface root={tree({ id: `m`, component: `Mosaic`, props: { pictures, label: `Shots` } } as unknown as NestedNode)} theme="neutral" id="m1" />)
    const root = container.querySelector<HTMLElement>(`[data-xui-id="m"]`)!
    expect(root.getAttribute(`role`)).toBe(`group`)
    expect(rule(`m`)).toContain(`aspect-ratio:1.6`)
    const lead = container.querySelector<HTMLElement>(`[data-xui-id="m.lead"]`)!
    expect(rule(`m.lead`)).toContain(`flex-basis:60%`)
    expect(lead.getAttribute(`aria-label`)).toBe(`P1`)
    const image = container.querySelector<HTMLElement>(`[data-xui-id="m.lead.image"]`)!
    expect(rule(`m.lead.image`)).toContain(`position:absolute`)
    expect(rule(`m.lead.image`)).toMatch(/inset:0(px)?[;}]/)
    expect(rule(`m.lead.image`)).toContain(`width:100%`)
    expect(image.querySelector(`img`)!.style.objectFit).toBe(`cover`)
    expect(container.querySelector(`[data-xui-id="m.side.bottom.more.moreLabel"]`)!.textContent).toBe(`+2`)
    expect(container.querySelector(`[data-xui-id="m.side.bottom"]`)!.getAttribute(`aria-label`)).toBe(`P3, 2 more`)
  })
})

describe(`the primitive twins`, () => {
  it(`Outline: caption, one active row with the bar, counts, select`, () => {
    const onSelect = vi.fn()
    const { container } = render(<Outline caption="Plan" value="b" items={[{ id: `a`, title: `Alpha` }, { id: `b`, title: `Beta`, count: 3 }, { id: `c`, title: `Gamma`, count: 0 }]} onSelect={onSelect} />)
    const nav = container.querySelector(`nav`)!
    expect(nav.getAttribute(`aria-label`)).toBe(`Plan`)
    const rows = container.querySelectorAll(`[data-slot="outline-row"]`)
    expect(rows).toHaveLength(3)
    expect(rows[1]!.getAttribute(`data-active`)).toBe(`true`)
    expect(rows[1]!.getAttribute(`aria-current`)).toBe(`location`)
    expect(rows[1]!.getAttribute(`aria-label`)).toBe(`Beta, 3`)
    expect(rows[1]!.querySelector(`[data-slot="outline-bar"]`)).not.toBeNull()
    expect(rows[0]!.querySelector(`[data-slot="outline-bar"]`)).toBeNull()
    expect(rows[1]!.textContent).toBe(`Beta· 3`)
    expect(rows[2]!.textContent).toBe(`Gamma`)
    fireEvent.click(rows[2]!)
    expect(onSelect).toHaveBeenCalledWith(`c`)
  })

  it(`Mosaic: the tile layout per count, "+N" on the last, select by index`, () => {
    const pics = (n: number) => Array.from({ length: n }, (_, i) => ({ src: `/p${i}.png`, alt: `P${i}` }))
    const sizes = (n: number) => {
      const r = render(<Mosaic pictures={pics(n)} />)
      const out = [...r.container.querySelectorAll(`[data-slot="mosaic-tile"]`)].map((t) => t.getAttribute(`data-size`))
      r.unmount()
      return out
    }
    expect(sizes(0)).toEqual([])
    expect(sizes(1)).toEqual([`large`])
    expect(sizes(2)).toEqual([`half`, `half`])
    expect(sizes(3)).toEqual([`large`, `small`, `small`])
    const onSelect = vi.fn()
    const { container } = render(<Mosaic pictures={pics(6)} label="Shots" onSelect={onSelect} />)
    expect(container.querySelector(`[data-slot="mosaic"]`)!.getAttribute(`aria-label`)).toBe(`Shots`)
    expect(container.querySelector(`[data-slot="mosaic-side"]`)!.children).toHaveLength(2)
    const tiles = container.querySelectorAll(`[data-slot="mosaic-tile"]`)
    expect(tiles[2]!.textContent).toBe(`+3`)
    expect(tiles[2]!.getAttribute(`aria-label`)).toBe(`P2, 3 more`)
    fireEvent.click(tiles[1]!)
    expect(onSelect).toHaveBeenCalledWith(1)
  })

  it(`CarouselCounter + CarouselThumbnails`, () => {
    const onPageChange = vi.fn()
    const { container } = render(
      <>
        <CarouselCounter page={0} total={4} />
        <CarouselThumbnails count={3} urls={[`/a.png`, null, `/c.png`, `/d.png`]} page={1} onPageChange={onPageChange} />
      </>
    )
    const counter = container.querySelector(`[data-slot="carousel-counter"]`)!
    expect(counter.textContent).toBe(`1 / 4`)
    expect(counter.getAttribute(`aria-hidden`)).toBe(`true`)
    const thumbs = container.querySelectorAll(`[data-slot="carousel-thumbnail"]`)
    expect(thumbs).toHaveLength(3)
    expect(thumbs[1]!.getAttribute(`data-current`)).toBe(`true`)
    expect(thumbs[1]!.getAttribute(`aria-current`)).toBe(`true`)
    expect(thumbs[1]!.querySelector(`img`)).toBeNull()
    expect(thumbs[2]!.getAttribute(`aria-label`)).toBe(`Page 3 of 3`)
    fireEvent.click(thumbs[0]!)
    expect(onPageChange).toHaveBeenCalledWith(0)
  })
})
