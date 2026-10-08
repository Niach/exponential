import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import {
  Carousel,
  CarouselContent,
  CarouselItem,
  CarouselNext,
  CarouselPrevious,
  type CarouselApi,
} from "./carousel"

describe(`Carousel`, () => {
  it(`renders a scroll-snap viewport with slides and hands out its api`, () => {
    const setApi = vi.fn<(api: CarouselApi) => void>()
    const { container, getAllByRole } = render(
      <Carousel setApi={setApi}>
        <CarouselContent>
          <CarouselItem>1</CarouselItem>
          <CarouselItem>2</CarouselItem>
        </CarouselContent>
        <CarouselPrevious />
        <CarouselNext />
      </Carousel>
    )
    expect(container.querySelector(`[data-slot="carousel"]`)).not.toBeNull()
    const viewport = container.querySelector(`[data-slot="carousel-content"]`)
    expect(viewport?.className).toContain(`snap-x`)
    expect(getAllByRole(`group`)).toHaveLength(2)
    expect(container.querySelectorAll(`[data-slot="carousel-item"]`)).toHaveLength(2)
    // jsdom has no layout: nothing to scroll, so both arrows sit disabled.
    expect(container.querySelector<HTMLButtonElement>(`[data-slot="carousel-previous"]`)?.disabled).toBe(true)
    expect(container.querySelector<HTMLButtonElement>(`[data-slot="carousel-next"]`)?.disabled).toBe(true)
    expect(setApi).toHaveBeenCalledOnce()
    expect(setApi.mock.calls[0][0].viewport).toBe(viewport)
  })
})
