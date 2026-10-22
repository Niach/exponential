// FEED-131: the Solid ScrollArea (React's round1-primitives ScrollArea case,
// plus Radix's hover scrollbar).
import { describe, expect, it } from "vitest"
import { fireEvent, render } from "@solidjs/testing-library"
import { ScrollArea } from "./scroll-area"

describe(`ScrollArea`, () => {
  it(`renders its slots; the viewport scrolls vertically once the bar mounts`, () => {
    const { container } = render(() => <ScrollArea class="h-10">body</ScrollArea>)
    const root = container.querySelector(`[data-slot="scroll-area"]`) as HTMLElement
    const viewport = container.querySelector(`[data-slot="scroll-area-viewport"]`) as HTMLElement
    expect(viewport.textContent).toBe(`body`)
    expect(root.getAttribute(`dir`)).toBe(`ltr`)
    expect(root.style.getPropertyValue(`--radix-scroll-area-corner-width`)).toBe(`0px`)
    expect(viewport.style.overflowY).toBe(`scroll`)
    expect(viewport.style.overflowX).toBe(`hidden`)
    expect(viewport.firstElementChild!.getAttribute(`style`)).toBe(`min-width: 100%; display: table;`)
  })

  it(`type="hover": no scrollbar until the pointer enters (and jsdom never overflows)`, () => {
    const { container } = render(() => <ScrollArea>body</ScrollArea>)
    const root = container.querySelector(`[data-slot="scroll-area"]`) as HTMLElement
    expect(container.querySelector(`[data-slot="scroll-area-scrollbar"]`)).toBeNull()
    fireEvent.pointerEnter(root)
    expect(container.querySelector(`[data-slot="scroll-area-scrollbar"]`)).toBeNull()
  })

  it(`type="always" paints the bar at once, sized from Radix's 18px minimum`, () => {
    const { container } = render(() => <ScrollArea type="always">body</ScrollArea>)
    const bar = container.querySelector(`[data-slot="scroll-area-scrollbar"]`) as HTMLElement
    expect(bar.getAttribute(`data-orientation`)).toBe(`vertical`)
    expect(bar.getAttribute(`style`)).toBe(`position: absolute; top: 0px; right: 0px; bottom: var(--radix-scroll-area-corner-height); --radix-scroll-area-thumb-height: 18px;`)
    expect(container.querySelector(`[data-slot="scroll-area-thumb"]`)).toBeNull()
  })
})
