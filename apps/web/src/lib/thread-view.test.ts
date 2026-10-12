import { describe, expect, it } from "vitest"
import fixtureJson from "@exp/domain-contract/fixtures/thread-view.json"
import {
  buildThreadView,
  partitionThreadList,
  sessionGuideEntries,
  threadPrTarget,
} from "@/lib/thread-view"

// EXP-1284: the thread view model is fixture-locked ×4; every client runs the
// same cases from packages/domain-contract/fixtures/thread-view.json.
const fixture = fixtureJson as unknown as {
  views: Array<{ name: string; input: never; expected: unknown }>
  partitions: Array<{
    name: string
    input: never
    scope: { teamId: string | null }
    expected: unknown
  }>
  prTargets: Array<{ name: string; input: never; expected: unknown }>
  guideSlices: Array<{
    name: string
    input: { results: never; sessionId: string; firstSessionId: string | null }
    expected: unknown
  }>
}

describe(`buildThreadView (thread-view.json)`, () => {
  for (const c of fixture.views) {
    it(c.name, () => {
      expect(buildThreadView(c.input)).toEqual(c.expected)
    })
  }
})

describe(`partitionThreadList (thread-view.json)`, () => {
  for (const c of fixture.partitions) {
    it(c.name, () => {
      expect(partitionThreadList(c.input, c.scope)).toEqual(c.expected)
    })
  }
})

describe(`threadPrTarget (thread-view.json)`, () => {
  for (const c of fixture.prTargets) {
    it(c.name, () => {
      expect(threadPrTarget(c.input)).toEqual(c.expected)
    })
  }
})

describe(`sessionGuideEntries (thread-view.json)`, () => {
  for (const c of fixture.guideSlices) {
    it(c.name, () => {
      expect(
        sessionGuideEntries(c.input.results, c.input.sessionId, c.input.firstSessionId)
      ).toEqual(c.expected)
    })
  }
})
