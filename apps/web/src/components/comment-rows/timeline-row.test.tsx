import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { TimelineRow } from "@/components/comment-rows/timeline-row"

// The rail is the whole point of the row: the feed's first entry must not draw
// a line reaching up into the "Activity" heading, and the last must not draw
// one down into the composer (EXP-698 r5).
function rails(container: HTMLElement) {
  return {
    above: container.querySelector(`[data-timeline-rail="above"]`),
    below: container.querySelector(`[data-timeline-rail="below"]`),
  }
}

describe(`TimelineRow`, () => {
  it(`draws both rail segments by default`, () => {
    const { container } = render(
      <TimelineRow marker={<span>dot</span>} markerSize={14}>
        body
      </TimelineRow>
    )
    const { above, below } = rails(container)
    expect(above).not.toBeNull()
    expect(below).not.toBeNull()
  })

  it(`drops the segment each flag turns off`, () => {
    const first = render(
      <TimelineRow marker={<span>dot</span>} markerSize={14} lineAbove={false}>
        body
      </TimelineRow>
    )
    expect(rails(first.container).above).toBeNull()
    expect(rails(first.container).below).not.toBeNull()

    const last = render(
      <TimelineRow marker={<span>dot</span>} markerSize={14} lineBelow={false}>
        body
      </TimelineRow>
    )
    expect(rails(last.container).above).not.toBeNull()
    expect(rails(last.container).below).toBeNull()
  })

  it(`breaks the rail around the marker rather than running through it`, () => {
    const { container } = render(
      <TimelineRow marker={<span>dot</span>} markerSize={28} markerTop={2}>
        body
      </TimelineRow>
    )
    const below = container.querySelector<HTMLElement>(
      `[data-timeline-rail="below"]`
    )
    // 2 (markerTop) + 28 (marker) + 6 (the break) — the segment starts BELOW
    // the marker, never at its top edge.
    expect(below?.style.top).toBe(`36px`)
  })

  // The regression this pins: an event row with too little padding computed a
  // NEGATIVE above-segment, so the rail vanished between two consecutive
  // events and the feed lost its spine (EXP-698 r5 review).
  it(`still draws both segments at the event row's geometry`, () => {
    const { container } = render(
      <TimelineRow marker={<span>glyph</span>} markerSize={14} markerTop={3}>
        moved this to Done
      </TimelineRow>
    )
    const above = container.querySelector<HTMLElement>(
      `[data-timeline-rail="above"]`
    )
    const below = container.querySelector<HTMLElement>(
      `[data-timeline-rail="below"]`
    )
    // 3 (markerTop) + 6 (padY) - 6 (the break): a real segment, not a
    // clamped-to-zero one.
    expect(above?.style.height).toBe(`3px`)
    // It bleeds the full padding, so it ends exactly where the next row's
    // above-segment starts.
    expect(above?.style.top).toBe(`-6px`)
    expect(below?.style.top).toBe(`23px`)
    expect(below?.style.bottom).toBe(`-6px`)
  })

  it(`renders the marker and the content side by side`, () => {
    const { container, getByText } = render(
      <TimelineRow marker={<span>M</span>} markerSize={14}>
        <span>content</span>
      </TimelineRow>
    )
    expect(container.querySelector(`[data-timeline-marker]`)?.textContent).toBe(
      `M`
    )
    expect(getByText(`content`)).toBeTruthy()
  })
})
