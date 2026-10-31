// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/upstream.json" with { type: "json" }
import { missingUpstreams, upstreamCallable, upstreamStep, type UpstreamEvent, type UpstreamState } from "./upstream"

test(`every state has a label`, () => {
  expect(Object.keys(fixture.labels).sort()).toEqual([...fixture.states].sort())
})

describe(`upstreamStep`, () => {
  for (const entry of fixture.flowCases) {
    test(entry.name, () => {
      const state = (entry.events as UpstreamEvent[]).reduce(upstreamStep, `disconnected` as UpstreamState)
      expect(state).toBe(entry.expected as UpstreamState)
    })
  }
})

describe(`upstreamCallable`, () => {
  for (const entry of fixture.callableCases) {
    test(entry.state, () => expect(upstreamCallable(entry.state as UpstreamState)).toBe(entry.expected))
  }
})

describe(`missingUpstreams`, () => {
  for (const entry of fixture.missingCases) {
    test(entry.name, () => {
      expect(missingUpstreams(entry.declared, entry.states as Record<string, UpstreamState>, entry.servers)).toEqual(entry.expected)
    })
  }
})
