import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { SegmentedBar, SEGMENT_TONE_CLASS, segmentToneClass } from "./segmented-bar"

// EXP-1051: the two rules the ×4 mirrors share — slices are drawn at their own
// width and CLIPPED rather than rescaled, and an unknown tone falls back to
// `neutral` instead of vanishing out of the geometry.

const slices = (container: HTMLElement) =>
  Array.from(
    container.querySelectorAll<HTMLElement>(`[data-slot="segmented-bar-segment"]`)
  )

describe(`SegmentedBar`, () => {
  it(`draws each segment at its own percent, in order`, () => {
    const { container } = render(
      <SegmentedBar
        segments={[
          { key: `base`, tone: `neutral`, percent: 10.5 },
          { key: `tools`, tone: `green`, percent: 1.2 },
          { key: `conversation`, tone: `blue`, percent: 14.45 },
        ]}
      />
    )
    expect(slices(container).map((node) => node.dataset.key)).toEqual([
      `base`,
      `tools`,
      `conversation`,
    ])
    expect(slices(container).map((node) => node.style.width)).toEqual([
      `10.5%`,
      `1.2%`,
      `14.45%`,
    ])
    expect(slices(container)[2].className).toContain(SEGMENT_TONE_CLASS.blue)
  })

  it(`never rescales an overshooting layout — it clips`, () => {
    const { container } = render(
      <SegmentedBar
        segments={[
          { key: `base`, tone: `neutral`, percent: 80 },
          { key: `project`, tone: `orange`, percent: 60 },
        ]}
      />
    )
    // The widths stay exactly what was asked for, and the track hides the rest.
    expect(slices(container).map((node) => node.style.width)).toEqual([
      `80%`,
      `60%`,
    ])
    for (const slice of slices(container)) {
      expect(slice.className).toContain(`shrink-0`)
    }
    expect(
      container.querySelector(`[data-slot="segmented-bar"]`)?.className
    ).toContain(`overflow-hidden`)
  })

  it(`lays a hairline at every tick`, () => {
    const { container } = render(
      <SegmentedBar segments={[]} ticks={[50, 75, 95]} />
    )
    const ticks = Array.from(
      container.querySelectorAll<HTMLElement>(`[data-slot="segmented-bar-tick"]`)
    )
    expect(ticks.map((node) => node.style.left)).toEqual([`50%`, `75%`, `95%`])
  })

  it(`falls an unknown tone back to neutral rather than dropping the slice`, () => {
    expect(segmentToneClass(`chartreuse`)).toBe(SEGMENT_TONE_CLASS.neutral)
    const { container } = render(
      <SegmentedBar segments={[{ key: `x`, tone: `chartreuse`, percent: 5 }]} />
    )
    expect(slices(container)).toHaveLength(1)
    expect(slices(container)[0].className).toContain(SEGMENT_TONE_CLASS.neutral)
  })
})
