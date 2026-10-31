import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { STORIES_TEST_IDS, STORY_TAB_SPECIMEN, storyFrameIndex, storyFrames } from "@exp/domain-contract/stories"
import fixture from "@exp/domain-contract/fixtures/stories.json"

import {
  STORY_RING_SPECIMEN,
  STORY_SPECIMEN_INPUT,
  STORY_SPECIMEN_PROGRESS,
  STORY_SPECIMEN_SMALL_RING,
  STORY_VIEWER_SPECIMEN_CARD,
  STORY_VIEWER_SPECIMEN_RING,
  StoryRingSpecimen,
  StoryViewerSpecimen,
} from "./stories-specimen"
import {
  stepFrame,
  storyArcClass,
  storyArcs,
  storyCardDetail,
  StoryProgress,
  StoryViewer,
  StoryViewerPanel,
} from "./stories"

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

describe(`stepFrame`, () => {
  const rings = STORY_RING_SPECIMEN
  const at = (ring: number, storyId: string) => ({ ringKey: rings[ring]!.key, storyId, picture: 0 })
  it(`walks a ring's stories, then the next ring; past the last ring closes`, () => {
    expect(stepFrame(rings, at(1, `triage-plan`), 1)).toEqual(at(1, `triage-result`))
    expect(stepFrame(rings, at(1, `triage-result`), 1)).toEqual(at(2, `designer-result`))
    expect(stepFrame(rings, at(3, `ship-result`), 1)).toBeNull()
  })
  it(`back from a ring's first story = the previous ring's last; the very first stays`, () => {
    expect(stepFrame(rings, at(2, `designer-result`), -1)).toEqual(at(1, `triage-result`))
    expect(stepFrame(rings, at(0, `board-question`), -1)).toBe(`stay`)
  })
  it(`#EXP-1493: a picture story is ONE step (its pictures swipe inside it)`, () => {
    const ring = STORY_TAB_SPECIMEN
    const index = ring.stories.findIndex((story) => story.pictures.length > 1)
    const pictured = ring.stories[index]!
    const next = ring.stories[index + 1]
    const step = stepFrame([ring], { ringKey: ring.key, storyId: pictured.id }, 1)
    expect(step).toEqual(next ? { ringKey: ring.key, storyId: next.id, picture: 0 } : null)
  })
})

describe(`StoryProgress`, () => {
  it(`done segments full, the current at its fraction, the rest empty, progress.gap apart`, () => {
    const { container } = render(<StoryProgress count={3} index={1} fraction={0.5} />)
    const segments = [...container.querySelectorAll(`[data-slot="story-progress-segment"]`)] as HTMLElement[]
    expect(segments.map((node) => (node.firstElementChild as HTMLElement).style.width)).toEqual([`100%`, `50%`, `0%`])
    expect(segments.map((node) => node.style.marginLeft)).toEqual([`0px`, `${fixture.progress.gap}px`, `${fixture.progress.gap}px`])
  })
})

describe(`StoryViewer frames`, () => {
  const ring = STORY_TAB_SPECIMEN
  const frames = storyFrames(ring)
  const pictured = ring.stories.find((story) => story.pictures.length > 1)!
  const viewer = (props: Partial<Parameters<typeof StoryViewer>[0]> = {}) => (
    <StoryViewer
      rings={[ring]}
      openRing={ring.key}
      onClose={() => {}}
      avatarFor={() => null}
      cardFor={() => null}
      {...props}
    />
  )

  it(`a thumbnail opens its story's ONE gallery frame at that picture: one segment per story, the badge, the caption under the picture`, () => {
    render(viewer({ openStory: pictured.id, openPicture: 1 }))
    const panel = screen.getByTestId(STORIES_TEST_IDS.viewer)
    expect(panel.dataset.frame).toBe(`gallery`)
    expect(screen.getByTestId(STORIES_TEST_IDS.progress).dataset.index).toBe(String(storyFrameIndex(frames, pictured.id)))
    expect(panel.querySelectorAll(`[data-slot="story-progress-segment"]`)).toHaveLength(ring.stories.length)
    expect(panel.querySelector(`[data-slot="story-badge"]`)!.getAttribute(`data-kind`)).toBe(pictured.kind)
    expect(panel.querySelector(`[data-slot="carousel-counter"]`)!.textContent).toBe(`2 / ${pictured.pictures.length}`)
    expect(panel.querySelectorAll(`[data-slot="carousel-thumbnails"] button`)).toHaveLength(pictured.pictures.length)
    expect(panel.querySelector(`[data-slot="story-caption"]`)!.textContent).toBe(
      pictured.pictures[1]!.caption ?? pictured.pictures[1]!.label
    )
  })

  it(`the side arrows step STORIES and never close`, () => {
    const onClose = vi.fn()
    const at = storyFrameIndex(frames, pictured.id)
    render(viewer({ openStory: pictured.id, onClose }))
    fireEvent.click(screen.getByTestId(STORIES_TEST_IDS.next))
    expect(onClose).not.toHaveBeenCalled()
    expect(screen.getByTestId(STORIES_TEST_IDS.progress).dataset.index).toBe(String(Math.min(at + 1, frames.length - 1)))
    fireEvent.click(screen.getByTestId(STORIES_TEST_IDS.previous))
    expect(screen.getByTestId(STORIES_TEST_IDS.progress).dataset.index).toBe(String(at))
  })

  it(`a plan = its cover with Open plan (the viewer closes, the host opens the page)`, () => {
    const plan = ring.stories.find((story) => story.kind === `plan`)!
    const onOpenPlan = vi.fn()
    const onClose = vi.fn()
    render(viewer({ openStory: plan.id, onOpenPlan, onClose }))
    expect(screen.getByTestId(STORIES_TEST_IDS.viewer).dataset.frame).toBe(`plan`)
    fireEvent.click(screen.getByTestId(`open-plan`))
    expect(onClose).toHaveBeenCalled()
    expect(onOpenPlan).toHaveBeenCalledWith(expect.objectContaining({ id: plan.id }))
  })

  it(`ArrowRight past the last story of the last ring closes`, () => {
    const onClose = vi.fn()
    render(viewer({ openStory: frames.at(-1)!.storyId, onClose }))
    fireEvent.keyDown(screen.getByTestId(STORIES_TEST_IDS.viewer), { key: `ArrowRight` })
    expect(onClose).toHaveBeenCalled()
  })
})

describe(`specimens`, () => {
  it(`the specimen = the fold of stories.json specimen.input, in fold order`, () => {
    expect(STORY_SPECIMEN_INPUT).toEqual(fixture.specimen.input as never)
    expect(STORY_RING_SPECIMEN.map((ring) => ring.title)).toEqual([
      `Board header polish`,
      `Triage`,
      `Designer`,
      `Ship the launch checklist`,
    ])
    expect(STORY_SPECIMEN_SMALL_RING).toBe(fixture.specimen.smallRing)
    expect(STORY_SPECIMEN_PROGRESS).toEqual(fixture.specimen.progress)
  })

  it(`the ring row draws one ring per ring, an arc per story; then the small rings and a progress; no state words`, () => {
    const { container } = render(<StoryRingSpecimen />)
    const rings = screen.getAllByTestId(STORIES_TEST_IDS.ring)
    expect(rings.map((ring) => ring.getAttribute(`data-key`))).toEqual(STORY_RING_SPECIMEN.map((ring) => ring.key))
    expect(rings[1]!.querySelectorAll(`[data-slot="story-arc"]`)).toHaveLength(2)
    const small = container.querySelector(`[data-slot="story-ring-specimen-small"]`)!
    expect(small.querySelectorAll(`[data-slot="story-ring"]`)).toHaveLength(4)
    expect(small.textContent).toBe(``)
    expect(screen.getByTestId(STORIES_TEST_IDS.progress).getAttribute(`data-index`)).toBe(`1`)
    expect(container.textContent).not.toMatch(/waiting|unseen|seen/)
  })

  it(`the viewer opens on ring 0, frame 0: the board question's bare surface, its title once, progress at 0, the two choices`, () => {
    render(<StoryViewerSpecimen />)
    const viewer = screen.getByTestId(STORIES_TEST_IDS.viewer)
    expect(viewer.getAttribute(`data-kind`)).toBe(`question`)
    const fills = [...screen.getByTestId(STORIES_TEST_IDS.progress).querySelectorAll(`[data-slot="story-progress-segment"] > span`)]
    expect(fills.map((node) => (node as HTMLElement).style.width)).toEqual([`0%`])
    expect(screen.getByTestId(STORIES_TEST_IDS.header).textContent).toContain(`Board header polish`)
    expect(viewer.dataset.frame).toBe(`surface`)
    expect(viewer.querySelector(`[data-slot="story-title"]`)!.textContent).toBe(`Which accent for the board header?`)
    expect(viewer.querySelector(`[data-slot="story-surface"]`)).toBeTruthy()
    expect(viewer.querySelector(`[data-footer="choices"]`)).toBeTruthy()
  })

  it(`a tap on the header opens the posting session; the close glyph closes`, () => {
    const onOpenSession = vi.fn()
    const onClose = vi.fn()
    const ring = STORY_VIEWER_SPECIMEN_RING
    render(
      <StoryViewerPanel
        ring={ring}
        index={0}
        fraction={1}
        avatar={null}
        detail={storyCardDetail(STORY_VIEWER_SPECIMEN_CARD)}
        onOpenSession={onOpenSession}
        onClose={onClose}
      />
    )
    fireEvent.click(screen.getByText(`Board header polish`))
    expect(onOpenSession).toHaveBeenCalledWith(ring.stories[0]!.sessionId)
    fireEvent.click(screen.getByLabelText(`Close`))
    expect(onClose).toHaveBeenCalled()
  })
})
