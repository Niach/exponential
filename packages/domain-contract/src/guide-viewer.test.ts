import { describe, expect, test } from "bun:test"

import results from "../fixtures/session-results.json" with { type: "json" }
import { guideViewer } from "./guide-viewer"

describe(`guide viewer fixtures (EXP-1283)`, () => {
  for (const entry of results.viewer.cases) {
    test(entry.name, () => {
      expect(guideViewer(entry.guide as never, entry.attachmentId)).toEqual(entry.expected as never)
    })
  }
})
