import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/live-activity.json" with { type: "json" }
import { liveActivityContent, liveActivityFollows } from "./live-activity"

describe(`liveActivityFollows`, () => {
  for (const entry of fixture.followCases) {
    test(entry.name, () => expect(liveActivityFollows(entry.input as never)).toEqual(entry.expected))
  }
})

describe(`liveActivityContent`, () => {
  for (const entry of fixture.contentCases) {
    test(entry.name, () => expect(liveActivityContent(entry.input as never)).toEqual(entry.expected))
  }
})
