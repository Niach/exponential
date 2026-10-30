// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/stories.json" with { type: "json" }
import { bubbleRings, stories, storyFrames, storyTab } from "./stories"

describe(`stories`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(stories(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})

describe(`bubbleRings`, () => {
  test(`the specimen's agent rings ride chats.json bubbleCases[0]'s bubbles, the chat rings follow`, () => {
    const rings = stories(fixture.specimen.input as never)
    const { byAgent, rest } = bubbleRings([`as`, `ag1`, `ag2`, `ag3`], rings)
    expect([...byAgent.keys()].sort()).toEqual([`ag1`, `ag2`])
    expect(byAgent.get(`ag1`)?.key).toBe(`agent:ag1`)
    expect(rest.map((ring) => ring.key)).toEqual(rings.filter((ring) => !ring.agentId).map((ring) => ring.key))
  })
  test(`an agent ring with no bubble follows in ring order`, () => {
    const rings = stories(fixture.specimen.input as never)
    const { byAgent, rest } = bubbleRings([`ag2`], rings)
    expect([...byAgent.keys()]).toEqual([`ag2`])
    expect(rest.map((ring) => ring.key)).toEqual(rings.filter((ring) => ring.agentId !== `ag2`).map((ring) => ring.key))
  })
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
