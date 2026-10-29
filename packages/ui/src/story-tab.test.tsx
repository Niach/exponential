import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { STORY_TAB, STORY_TAB_SPECIMEN } from "@exp/domain-contract/stories"

import { StoryTab, storyThumbWidth } from "./story-tab"
import { StoryTabSpecimen } from "./story-tab-specimen"

// STORIES TAB (stories.json): the tiles paint `storyTab()`'s ring.
describe(`storyThumbWidth`, () => {
  it(`by aspect at the thumb height, capped; unknown = 4:3`, () => {
    const { thumbHeight, thumbMaxWidth } = STORY_TAB.tile
    expect(storyThumbWidth({ width: 1179, height: 2556 })).toBe(Math.round(thumbHeight * (1179 / 2556)))
    expect(storyThumbWidth({ width: 4000, height: 1000 })).toBe(thumbMaxWidth)
    expect(storyThumbWidth({ width: null, height: null })).toBe(Math.round((thumbHeight * 4) / 3))
  })
})

describe(`StoryTab`, () => {
  it(`one tile per story in fold order, the title + meta, a thumbnail per picture with its caption else label`, () => {
    render(<StoryTab ring={STORY_TAB_SPECIMEN} attachmentSrc={(id) => `/a/${id}`} />)
    const tiles = screen.getAllByTestId(STORY_TAB.testIds.tile)
    expect(tiles.map((tile) => tile.dataset.story)).toEqual(STORY_TAB_SPECIMEN.stories.map((story) => story.id))
    expect(tiles[0]!.dataset.waiting).toBe(`true`)
    expect(tiles[0]!.textContent).toContain(`Ship the picker as is?`)
    expect(tiles[0]!.textContent).toContain(`Question · 10m`)
    const pictures = tiles[1]!.querySelectorAll(`[data-slot="story-tile-picture"]`)
    expect(pictures).toHaveLength(2)
    expect(pictures[0]!.textContent).toBe(`Mixed state`)
    expect(pictures[1]!.textContent).toBe(`ios picker`)
    expect(tiles[2]!.textContent).toContain(`Question · Answered · 3h`)
  })

  it(`a tap opens the story; a thumbnail opens its picture`, () => {
    const onOpen = vi.fn()
    render(<StoryTab ring={STORY_TAB_SPECIMEN} onOpen={onOpen} />)
    const tiles = screen.getAllByTestId(STORY_TAB.testIds.tile)
    fireEvent.click(tiles[0]!.querySelector(`button`)!)
    expect(onOpen).toHaveBeenLastCalledWith(STORY_TAB_SPECIMEN.stories[0], 0)
    fireEvent.click(tiles[1]!.querySelectorAll(`[data-slot="story-tile-picture"]`)[1]!)
    expect(onOpen).toHaveBeenLastCalledWith(STORY_TAB_SPECIMEN.stories[1], 1)
  })

  it(`no ring = THE empty state, the lead still above it`, () => {
    render(<StoryTab ring={null} lead={<div data-testid="lead" />} />)
    expect(screen.getByTestId(`lead`)).toBeTruthy()
    expect(screen.getByText(STORY_TAB.copy.empty)).toBeTruthy()
  })

  it(`the specimen draws tabCases[tab.specimen.case]`, () => {
    render(<StoryTabSpecimen />)
    expect(screen.getAllByTestId(STORY_TAB.testIds.tile)).toHaveLength(STORY_TAB_SPECIMEN.stories.length)
  })
})

describe(`StoryViewer openStory`, () => {
  it(`opens on the tapped story, not the ring's first`, async () => {
    const { StoryViewer } = await import(`./stories`)
    const ring = STORY_TAB_SPECIMEN
    const third = ring.stories[2]!
    render(
      <StoryViewer
        rings={[ring]}
        openRing={ring.key}
        openStory={third.id}
        onClose={() => {}}
        avatarFor={() => null}
        cardFor={() => null}
      />
    )
    expect(screen.getByTestId(`story-header`).textContent).toContain(third.meta)
  })
})
