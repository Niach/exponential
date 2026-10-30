// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/stories.json" with { type: "json" }
import { stories, storyFrames, storyRow, storyTab, storyViewerSize } from "./stories"

describe(`stories`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(stories(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})

describe(`storyRow`, () => {
  for (const entry of fixture.rowCases) {
    test(entry.name, () => {
      const { bubbles, rest } = storyRow(entry.input.bubbleAgentIds, entry.input.rings as never)
      expect({ bubbles, rest: rest.map((ring) => ring.key) }).toEqual(entry.expected)
    })
  }
})

describe(`storyViewerSize`, () => {
  for (const entry of fixture.sizeCases) {
    test(entry.name, () => {
      expect(storyViewerSize(entry.input.picture, entry.input.viewport)).toEqual(entry.expected)
    })
  }
})

describe(`storyTab`, () => {
  for (const entry of fixture.tabCases) {
    test(entry.name, () => {
      expect(storyTab(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})

describe(`storyFrames`, () => {
  for (const entry of fixture.frameCases) {
    test(entry.name, () => {
      expect(storyFrames(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})
