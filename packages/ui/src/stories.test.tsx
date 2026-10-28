import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { STORIES_TEST_IDS } from "@exp/domain-contract/stories"

import {
  STORY_RING_SPECIMEN,
  STORY_VIEWER_SPECIMEN_CARD,
  STORY_VIEWER_SPECIMEN_RING,
  StoryRingSpecimen,
  StoryViewerSpecimen,
} from "./stories-specimen"
import { stepStory, storyArcClass, storyArcs, storyCardDetail, StoryProgress, StoryViewerPanel } from "./stories"

describe(`storyArcs`, () => {
  it(`one story = the whole circle; several = equal slots, gap apart`, () => {
    expect(storyArcs(0)).toEqual([])
    expect(storyArcs(1)).toEqual([[0, 360]])
    expect(storyArcs(3, 6)).toEqual([
      [3, 114],
      [123, 114],
      [243, 114],
    ])
  })

  it(`tints: waiting = the plan/question tint, unseen = foreground, seen = the hairline`, () => {
    expect(storyArcClass({ kind: `question`, waiting: true, seen: false })).toBe(`stroke-yellow-400`)
    expect(storyArcClass({ kind: `plan`, waiting: true, seen: false })).toBe(`stroke-primary`)
    expect(storyArcClass({ kind: `result`, waiting: false, seen: false })).toBe(`stroke-foreground`)
    expect(storyArcClass({ kind: `result`, waiting: false, seen: true })).toBe(`stroke-glass-stroke`)
  })
})

describe(`stepStory`, () => {
  const rings = STORY_RING_SPECIMEN
  it(`walks a ring, then the next ring; past the last ring closes`, () => {
    expect(stepStory(rings, { ringKey: rings[0]!.key, storyId: `a1` }, 1)).toEqual({ ringKey: rings[0]!.key, storyId: `a2` })
    expect(stepStory(rings, { ringKey: rings[0]!.key, storyId: `a2` }, 1)).toEqual({ ringKey: rings[1]!.key, storyId: `q1` })
    expect(stepStory(rings, { ringKey: rings[3]!.key, storyId: `s1` }, 1)).toBeNull()
  })
  it(`back from a ring's first story = the previous ring's last; the very first stays`, () => {
    expect(stepStory(rings, { ringKey: rings[1]!.key, storyId: `q1` }, -1)).toEqual({ ringKey: rings[0]!.key, storyId: `a2` })
    expect(stepStory(rings, { ringKey: rings[0]!.key, storyId: `a1` }, -1)).toBe(`stay`)
  })
})

describe(`StoryProgress`, () => {
  it(`done segments full, the current at its fraction, the rest empty`, () => {
    const { container } = render(<StoryProgress count={3} index={1} fraction={0.5} />)
    const fills = [...container.querySelectorAll(`[data-slot="story-progress-segment"] > span`)].map(
      (node) => (node as HTMLElement).style.width
    )
    expect(fills).toEqual([`100%`, `50%`, `0%`])
  })
})

describe(`specimens`, () => {
  it(`the ring row draws one ring per ring, an arc per story`, () => {
    render(<StoryRingSpecimen />)
    const rings = screen.getAllByTestId(STORIES_TEST_IDS.ring)
    expect(rings.map((ring) => ring.getAttribute(`data-key`))).toEqual(STORY_RING_SPECIMEN.map((ring) => ring.key))
    expect(rings[0]!.querySelectorAll(`[data-slot="story-arc"]`)).toHaveLength(2)
  })

  it(`the viewer opens on the draft question: progress, header, the two choices`, () => {
    render(<StoryViewerSpecimen />)
    const viewer = screen.getByTestId(STORIES_TEST_IDS.viewer)
    expect(viewer.getAttribute(`data-kind`)).toBe(`question`)
    expect(screen.getByTestId(STORIES_TEST_IDS.progress)).toBeTruthy()
    expect(screen.getByTestId(STORIES_TEST_IDS.header).textContent).toContain(`Redesign the header`)
    expect(viewer.querySelector(`[data-footer="choices"]`)).toBeTruthy()
  })

  it(`a tap on the header opens the posting session; the close glyph closes`, () => {
    const onOpenSession = vi.fn()
    const onClose = vi.fn()
    const ring = STORY_VIEWER_SPECIMEN_RING
    render(
      <StoryViewerPanel
        ring={ring}
        story={ring.stories[0]!}
        index={0}
        fraction={1}
        avatar={null}
        detail={storyCardDetail(STORY_VIEWER_SPECIMEN_CARD)}
        onOpenSession={onOpenSession}
        onClose={onClose}
      />
    )
    fireEvent.click(screen.getByText(`Redesign the header`))
    expect(onOpenSession).toHaveBeenCalledWith(ring.stories[0]!.sessionId)
    fireEvent.click(screen.getByLabelText(`Close`))
    expect(onClose).toHaveBeenCalled()
  })
})
