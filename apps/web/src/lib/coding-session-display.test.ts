import { describe, expect, it } from "vitest"
import { sessionDisplayState } from "./coding-session-display"

// Parity suite — iOS CodingSessionDisplayTests.swift and Android
// CodingSessionDisplayTest.kt assert the same cases; move all of them in
// lockstep.
describe(`sessionDisplayState`, () => {
  it(`in_review with a merged PR is done (old-server tolerance)`, () => {
    expect(
      sessionDisplayState({ status: `in_review`, needsInput: false }, `merged`)
    ).toBe(`done`)
  })

  it(`in_review with an open PR is review`, () => {
    expect(
      sessionDisplayState({ status: `in_review`, needsInput: false }, `open`)
    ).toBe(`review`)
  })

  it(`in_review with needsInput stays review (EXP-531)`, () => {
    // The desktop's post-turn idle nudge lands AFTER the PR-open flip — a
    // reviewed session must never read "Needs input".
    expect(
      sessionDisplayState({ status: `in_review`, needsInput: true }, `open`)
    ).toBe(`review`)
  })

  it(`running with needsInput is needs_input`, () => {
    expect(
      sessionDisplayState({ status: `running`, needsInput: true }, null)
    ).toBe(`needs_input`)
  })

  it(`running without needsInput is running`, () => {
    expect(
      sessionDisplayState({ status: `running`, needsInput: false }, null)
    ).toBe(`running`)
  })
})
