import { describe, expect, it } from "vitest"
import display from "@exp/domain-contract/fixtures/session-display.json"
import {
  sessionAgentCaption,
  sessionDisplayState,
  sessionRowIsWorking,
  sessionStatusLine,
} from "./coding-session-display"

// EXP-1184: the ×4 rule, replayed from the shared fixture — desktop
// queries.rs, iOS CodingSessionDisplayTests.swift and Android
// CodingSessionDisplayTest.kt read the same cases.
describe(`session-display.json`, () => {
  for (const c of display.cases) {
    it(c.name, () => {
      const session = {
        status: c.status as `running` | `in_review` | `ended`,
        needsInput: c.needsInput,
        agentBusy: c.agentBusy,
      }
      const state = sessionDisplayState(session, c.prState)
      expect(state).toBe(c.state)
      expect(sessionRowIsWorking(session, c.prState)).toBe(c.working)
      expect(
        sessionStatusLine({ state, paused: false, device: `mbp`, startedAt: null })
          .tone
      ).toBe(c.statusTone)
    })
  }
})

// EXP-850 §8: the session row's second line.
describe(`sessionAgentCaption`, () => {
  it(`is the device-written caption of a live row`, () => {
    expect(
      sessionAgentCaption({
        status: `running`,
        agentCaption: `Workflow wire-probe · 2/3 agents done · Beta`,
      })
    ).toBe(`Workflow wire-probe · 2/3 agents done · Beta`)
    expect(
      sessionAgentCaption({ status: `in_review`, agentCaption: `Workflow x · stopped` })
    ).toBe(`Workflow x · stopped`)
  })

  it(`is null without one, and for a blank one`, () => {
    expect(sessionAgentCaption({ status: `running`, agentCaption: null })).toBeNull()
    expect(sessionAgentCaption({ status: `running`, agentCaption: `   ` })).toBeNull()
  })

  it(`an ended or merged row never narrates`, () => {
    expect(
      sessionAgentCaption({ status: `ended`, agentCaption: `Workflow x · 1/2 agents done` })
    ).toBeNull()
    expect(
      sessionAgentCaption({ status: `merged`, agentCaption: `Workflow x · done · 2 agents` })
    ).toBeNull()
  })
})

// EXP-874: the live row's status line (Android's row is the reference).
describe(`sessionStatusLine`, () => {
  const startedAt = new Date(Date.now() - 5 * 60_000)

  it(`a paused run says so, whatever its state`, () => {
    for (const state of [`working`, `needs_input`, `review`, `done`] as const) {
      expect(
        sessionStatusLine({ state, paused: true, device: `mbp`, startedAt })
      ).toEqual({ text: `Paused · mbp`, tone: `muted` })
    }
  })

  it(`a parked state leads with its label and tone`, () => {
    expect(
      sessionStatusLine({ state: `needs_input`, paused: false, device: `mbp`, startedAt })
    ).toEqual({ text: `Needs input · mbp`, tone: `amber` })
    expect(
      sessionStatusLine({ state: `review`, paused: false, device: `mbp`, startedAt })
    ).toEqual({ text: `Ready for review · mbp`, tone: `emerald` })
    expect(
      sessionStatusLine({ state: `done`, paused: false, device: `mbp`, startedAt })
    ).toEqual({ text: `Done · mbp`, tone: `sky` })
  })

  it(`a live run names the machine and when it started`, () => {
    expect(
      sessionStatusLine({ state: `working`, paused: false, device: `mbp`, startedAt })
    ).toEqual({ text: `mbp · started 5 minutes ago`, tone: `muted` })
  })
})
