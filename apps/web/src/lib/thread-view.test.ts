import { describe, expect, it } from "vitest"
import fixtureJson from "@exp/domain-contract/fixtures/thread-view.json"
import { buildThreadView, partitionThreadList } from "@/lib/thread-view"

// EXP-1284: the thread view model is fixture-locked ×4; every client runs the
// same cases from packages/domain-contract/fixtures/thread-view.json.
const fixture = fixtureJson as unknown as {
  views: Array<{ name: string; input: never; expected: unknown }>
  partitions: Array<{ name: string; input: never; expected: unknown }>
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
      expect(partitionThreadList(c.input)).toEqual(c.expected)
    })
  }
})
