// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/voice-call.json" with { type: "json" }
import {
  CALL_IDLE,
  callAvailability,
  callStep,
  callView,
  type CallAvailabilityInput,
  type CallEvent,
} from "./voice-call"

describe(`callAvailability`, () => {
  for (const entry of fixture.availabilityCases) {
    test(entry.name, () => {
      expect(callAvailability(entry.input as CallAvailabilityInput)).toEqual(entry.expected)
    })
  }
})

describe(`callStep + callView`, () => {
  for (const entry of fixture.flowCases) {
    test(entry.name, () => {
      const state = (entry.events as CallEvent[]).reduce(callStep, CALL_IDLE)
      expect(callView(state)).toEqual(entry.view as never)
      expect(state.toast).toBe((entry as { toast?: string }).toast ?? null)
    })
  }
})
